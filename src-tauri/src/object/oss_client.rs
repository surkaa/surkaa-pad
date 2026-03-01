use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::Utc;
use futures_util::TryStreamExt;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::io::Error;
use std::sync::Arc;

use crate::object::types::ByteStream;
use crate::object::NextToken;
use tauri::http::header::{CONTENT_TYPE, DATE};
use tauri::http::{HeaderMap, HeaderValue, Method};

const STREAM_MINE_TYPE: &str = "application/octet-stream";

#[derive(Debug, Eq, PartialEq)]
pub struct ObjectMetadata {
    key: String,
    size: u64,
    last_modified: chrono::DateTime<Utc>,
    etag: String,
}

impl ObjectMetadata {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn etag(&self) -> &str {
        &self.etag
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AliyunListObjectsResult {
    pub name: String,
    pub prefix: Option<String>,
    // pub marker: Option<String>,
    pub max_keys: i32,
    // pub is_truncated: bool,
    #[serde(rename = "Contents", default)] // 避免没有对象时反序列化失败
    pub contents: Vec<AliyunObjectSummary>,
    pub next_continuation_token: Option<String>,
}

impl AliyunListObjectsResult {
    pub fn from_xml(xml: String) -> Result<Self, quick_xml::DeError> {
        quick_xml::de::from_str(&xml)
    }
}

/// List Objects 中的单个对象摘要
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AliyunObjectSummary {
    pub key: String,
    pub last_modified: String,
    pub e_tag: String,
    pub size: u64,
    pub storage_class: String,
}

impl AliyunObjectSummary {
    fn to_object_metadata(self) -> ObjectMetadata {
        ObjectMetadata {
            key: self.key,
            size: self.size,
            last_modified: chrono::DateTime::parse_from_rfc3339(&self.last_modified)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            etag: self.e_tag.replace("\"", ""),
        }
    }
}

pub struct OssClientInner {
    endpoint: String,
    akid: String,
    sakey: String,
    bucket: String,
    http_client: reqwest::Client,
}

#[derive(Clone)]
pub struct OssClient {
    inner: Arc<OssClientInner>,
}

impl OssClient {
    pub fn new(endpoint: String, akid: String, sakey: String, bucket: String) -> Self {
        Self {
            inner: Arc::new(OssClientInner {
                endpoint,
                akid,
                sakey,
                bucket,
                http_client: reqwest::Client::new(),
            }),
        }
    }

    #[cfg(test)]
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        let akid = std::env::var("ALIYUN_KEY").expect("请配置ALIYUN_KEY");
        let sakey = std::env::var("ALIYUN_SECRET").expect("请配置ALIYUN_SECRET");
        let bucket = std::env::var("ALIYUN_BUCKET_NAME").expect("请配置ALIYUN_BUCKET_NAME");
        let endpoint = std::env::var("ALIYUN_ENDPOINT").expect("请配置ALIYUN_ENDPOINT");
        Self::new(endpoint, akid, sakey, bucket)
    }

    /// 构建完整的对象 URL
    fn get_url(&self, path: &str, query: &str) -> String {
        format!(
            "https://{}.{}/{}?{}",
            self.inner.bucket, self.inner.endpoint, path, query
        )
    }

    /// 构建签名请求头
    fn build_headers(
        &self,
        method: &Method,
        path: &str,
        content_type: &str,
    ) -> Result<HeaderMap, String> {
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

        // 签名字符串构造: VERB + \n + Content-MD5(可选) + \n + Content-Type + \n + Date + \n + CanonicalizedResource
        let canonicalized_resource = format!("/{}/{}", self.inner.bucket, path);
        let string_to_sign = format!(
            "{}\n\n{}\n{}\n{}",
            method.as_str(),
            content_type,
            date,
            canonicalized_resource
        );

        // HMAC-SHA1 签名
        type HmacSha1 = Hmac<Sha1>;
        let mut mac = HmacSha1::new_from_slice(self.inner.sakey.as_bytes())
            .map_err(|e| format!("非法长度{}", e))?;
        mac.update(string_to_sign.as_bytes());
        let signature = STANDARD.encode(mac.finalize().into_bytes());

        let mut headers = HeaderMap::new();
        headers.insert(
            DATE,
            HeaderValue::from_str(&date).map_err(|e| format!("未能创建date头:{}", e))?,
        );
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_str(content_type)
                .map_err(|e| format!("未能创建content-type头:{}", e))?,
        );
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("OSS {}:{}", self.inner.akid, signature))
                .map_err(|e| format!("未能创建authorization头:{}", e))?,
        );

        Ok(headers)
    }

    pub async fn upload(
        &self,
        key: &str,
        len: u64,
        stream: ByteStream,
        mimetype: &str,
    ) -> Result<(), String> {
        let url = self.get_url(key, "");
        let mut headers = self.build_headers(&Method::PUT, key, mimetype)?;
        // 显式设置 Content-Length
        headers.insert(reqwest::header::CONTENT_LENGTH, len.into());

        let stream_reader = reqwest::Body::wrap_stream(stream);

        let resp = self
            .inner
            .http_client
            .put(url)
            .headers(headers)
            .body(stream_reader)
            .send()
            .await
            .map_err(|e| format!("请求失败:{}", e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("上传失败 状态码:{}", resp.status()))
        }
    }

    pub async fn upload_bytes(&self, key: &str, data: &Vec<u8>) -> Result<(), String> {
        let url = self.get_url(key, "");
        let mut headers = self.build_headers(&Method::PUT, key, STREAM_MINE_TYPE)?;
        // 显式设置 Content-Length
        let len = data.len();
        headers.insert(reqwest::header::CONTENT_LENGTH, len.into());

        let resp = self
            .inner
            .http_client
            .put(url)
            .headers(headers)
            .body(data.clone())
            .send()
            .await
            .map_err(|e| format!("请求失败:{}", e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("上传失败 状态码:{}", resp.status()))
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), String> {
        let url = self.get_url(key, "");
        let headers = self.build_headers(&Method::DELETE, key, "")?;

        let resp = self
            .inner
            .http_client
            .delete(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| format!("请求失败:{}", e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("删除失败 {}", resp.status()))
        }
    }

    pub async fn delete_with_prefix(&self, prefix: &str) -> Result<u32, String> {
        // 列出所有匹配的对象
        let mut next_token: Option<String> = None;
        let mut needs_deletion = Vec::new();
        loop {
            let (objects, nt) = self.list(prefix, next_token).await?;
            for obj in objects {
                needs_deletion.push(obj.key().to_string());
            }
            if nt.is_none() {
                break;
            }
            next_token = nt;
        }
        let total = needs_deletion.len() as u32;
        // 逐个删除对象
        for key in needs_deletion {
            self.delete(&key).await?;
        }
        Ok(total)
    }

    pub async fn get_metadata(&self, key: &str) -> Result<ObjectMetadata, String> {
        let url = self.get_url(key, "");
        let headers = self.build_headers(&Method::HEAD, key, "")?;

        let resp = self
            .inner
            .http_client
            .head(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| format!("请求失败:{}", e))?;

        if !resp.status().is_success() {
            return Err(format!(
                "获取元信息失败 status:{}, key:{}",
                resp.status(),
                &key
            ));
        }

        let last_modified_str = resp
            .headers()
            .get("Last-Modified")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let last_modified = chrono::DateTime::parse_from_rfc2822(last_modified_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let etag: String = resp
            .headers()
            .get("ETag")
            .ok_or("响应缺少ETag头".to_string())?
            .to_str()
            .map_err(|e| format!("无法解析ETag头: {}", e))?
            .replace("\"", "");

        let size = resp
            .headers()
            .get("Content-Length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        Ok(ObjectMetadata {
            key: key.to_string(),
            size,
            last_modified,
            etag,
        })
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/listobjects-v2
    pub async fn list(
        &self,
        prefix: &str,
        next_token: NextToken,
    ) -> Result<(Vec<ObjectMetadata>, NextToken), String> {
        // 构造基础查询参数
        #[cfg(not(debug_assertions))]
        let mut query_params = format!("list-type=2&prefix={}&max-keys=1000", prefix);
        #[cfg(debug_assertions)] // 测试环境下单页为10个，方便测试分页逻辑
        let mut query_params = format!("list-type=2&prefix={}&max-keys=10", prefix);

        // 处理签名路径
        // 注意：OSS 要求 continuation-token 必须出现在签名字符串的 CanonicalizedResource 中
        let mut sign_path = String::new();
        if let Some(token) = next_token {
            let token_kv = format!("continuation-token={}", token);
            query_params.push_str(&format!("&{}", token_kv));
            sign_path.push_str(&format!("?{}", token_kv));
        }

        // 构造完整 URL
        let url = self.get_url("", &query_params);

        let headers = self.build_headers(&Method::GET, &sign_path, "")?;

        let resp = self
            .inner
            .http_client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| format!("未能发送列表请求到阿里云 OSS: {}", e))?;
        let xml_content = resp
            .text()
            .await
            .map_err(|e| format!("未能读取列表响应内容: {}", e))?;
        let aliyun_result = AliyunListObjectsResult::from_xml(xml_content)
            .map_err(|e| format!("未能解析列表响应 XML: {}", e))?;

        let next_token = aliyun_result.next_continuation_token;

        let objects = aliyun_result
            .contents
            .into_iter()
            .map(AliyunObjectSummary::to_object_metadata)
            .collect();
        Ok((objects, next_token))
    }

    pub async fn download(
        &self,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<(ByteStream, u64), String> {
        let url = self.get_url(key, "");
        let mut headers = self.build_headers(&Method::GET, key, "")?;

        // 1. 如果传入了 range，构建并插入标准的 HTTP Range 请求头
        if let Some((start, end)) = range {
            let range_val = format!("bytes={}-{}", start, end);
            headers.insert(
                reqwest::header::RANGE,
                HeaderValue::from_str(&range_val).map_err(|e| format!("未能创建Range头: {}", e))?,
            );
        }

        let resp = self
            .inner
            .http_client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| format!("请求失败:{}", e))?;

        if !resp.status().is_success() {
            return Err(format!("下载失败 {}, key:{}", resp.status(), &key));
        }

        let len = resp
            .headers()
            .get("Content-Length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let stream = resp
            .bytes_stream()
            .map_err(Error::other);

        Ok((Box::pin(stream), len))
    }

    pub async fn download_bytes(&self, key: &str) -> Result<Vec<u8>, String> {
        let url = self.get_url(key, "");
        let headers = self.build_headers(&Method::GET, key, "")?;

        let resp = self
            .inner
            .http_client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| format!("请求失败:{}", e))?;

        if !resp.status().is_success() {
            return Err(format!("下载失败 {}, key:{}", resp.status(), &key));
        }

        let data = resp
            .bytes()
            .await
            .map_err(|e| format!("读取响应数据失败: {}", e))?
            .to_vec();

        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures_util::stream::iter;
    use serial_test::serial;
    use std::iter::once;

    async fn assert_empty(client: &OssClient, msg: &str) {
        // 检查有没有遗留的测试文件
        let (objects, next_token) = client.list("", None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "{}", msg);
        if objects.len() != 0 {
            panic!("{}: 发现遗留对象 {:?}", msg, objects);
        }
    }

    async fn add_object(client: &OssClient, key: &str, content: &'static str) {
        let len = content.len() as u64;
        let bytes = Bytes::from_static(content.as_bytes());
        let stream: ByteStream = Box::pin(iter(once(Ok::<_, Error>(bytes))));
        client
            .upload(key, len, stream, STREAM_MINE_TYPE)
            .await
            .expect("上传失败");
    }

    #[serial]
    #[tokio::test]
    async fn test_oss() {
        let client = OssClient::from_env();
        let key = "test_upload.txt";
        let content = "This is a test line for OSS upload and download testing.";
        let repeat_count = 1000;

        assert_empty(&client, "测试开始前对象存储应为空").await;

        // 生成测试文件
        let dir = tempfile::tempdir().expect("无法创建临时目录");
        let file_path = dir.path().join(key);
        // 写入大量数据以测试上传
        let file_size = {
            let mut file = std::fs::File::create(&file_path).expect("无法创建测试文件");
            for _ in 0..repeat_count {
                use std::io::Write;
                writeln!(file, "{}", content).expect("无法写入测试文件");
            }
            // {}会自动关闭文件
            file.metadata().expect("无法获取文件元数据").len()
        };
        dbg!(&file_size);
        // 计算md5
        let file_content = std::fs::read(&file_path).expect("无法读取测试文件");
        let md5_etag = format!("{:X}", md5::compute(&file_content));

        // 上传文件
        let file = tokio::fs::File::open(file_path)
            .await
            .expect("无法打开测试文件");
        let mut uploaded: u64 = 0;
        let stream = tokio_util::io::ReaderStream::new(file).map_ok(move |chunk| {
            uploaded += chunk.len() as u64;
            let percentage = (uploaded as f64 / file_size as f64) * 100.0;
            println!(
                "🚀 上传进度: {:.2}% ({}/{})",
                percentage, uploaded, file_size
            );
            chunk
        });
        let stream: ByteStream = Box::pin(stream);
        client
            .upload(key, file_size, stream, STREAM_MINE_TYPE)
            .await
            .expect("上传失败");

        // 获取元数据
        let metadata = client.get_metadata(key).await.expect("获取元数据失败");
        assert_eq!(metadata.key, key);
        assert_eq!(metadata.size, file_size);
        assert_eq!(metadata.etag, md5_etag);
        let now = Utc::now();
        assert!(metadata.last_modified <= now);
        assert!(metadata.last_modified >= now - chrono::Duration::seconds(10));

        // 列出对象
        let (objects, next_token) = client.list("", None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "不应有续页");
        assert_eq!(objects.len(), 1, "应列出一个对象");
        assert_eq!(objects[0], metadata, "列出的元数据应匹配获取的元数据");

        // 下载对象
        let (mut download_stream, _) = client.download(key, None).await.expect("下载失败");
        let mut downloaded_data = Vec::new();
        while let Some(chunk) = download_stream.try_next().await.expect("读取下载流失败") {
            downloaded_data.extend_from_slice(&chunk);
        }
        assert_eq!(
            downloaded_data, file_content,
            "下载的数据应与上传的数据匹配"
        );

        // 删除对象
        client.delete(key).await.expect("删除失败");

        // 确认删除
        assert_empty(&client, "测试结束后对象存储应为空").await;
    }

    #[serial]
    #[tokio::test]
    async fn test_batch_delete() {
        let client = OssClient::from_env();
        assert_empty(&client, "测试开始前对象存储应为空").await;

        // 上传多个测试文件
        let prefix = "id_";
        let keys: Vec<String> = (0..5).map(|i| format!("{}{}", prefix, i)).collect();
        for key in &keys {
            let content = "This is a test file for batch delete.";
            let len = content.len() as u64;
            let bytes = Bytes::from_static(content.as_bytes());
            let stream: ByteStream = Box::pin(iter(once(Ok::<_, Error>(bytes))));
            client
                .upload(key, len, stream, STREAM_MINE_TYPE)
                .await
                .expect("上传失败");
        }

        // 确认上传
        let (objects, next_token) = client.list(prefix, None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "不应有续页");
        assert_eq!(objects.len(), keys.len(), "应列出所有上传的对象");
        dbg!(&objects);

        // 批量删除 使用通配符会删除失败
        client.delete("id_*").await.expect("批量删除失败");
        // 确认删除失败
        let (objects, next_token) = client.list(prefix, None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "不应有续页");
        assert_eq!(objects.len(), keys.len(), "对象不应被删除");

        // 使用前缀删除
        let delete_count = client
            .delete_with_prefix(prefix)
            .await
            .expect("前缀删除失败");
        assert_eq!(delete_count, keys.len() as u32, "应删除所有上传的对象");
        // 确认删除
        assert_empty(&client, "测试结束后对象存储应为空").await;
    }

    #[serial]
    #[tokio::test]
    async fn test_list() {
        let client = OssClient::from_env();
        assert_empty(&client, "测试开始前对象存储应为空").await;
        add_object(&client, "folder/test1.txt", "Test file 1").await;
        add_object(&client, "folder/test2.txt", "Test file 2").await;
        add_object(&client, "folder/subfolder/test3.txt", "Test file 3").await;

        // 列出对象
        let (objects, next_token) = client.list("", None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "不应有续页");
        assert_eq!(objects.len(), 3, "应列出三个对象");
        let keys: Vec<String> = objects.iter().map(|obj| obj.key().to_string()).collect();
        assert!(keys.contains(&"folder/test1.txt".to_string()));
        assert!(keys.contains(&"folder/test2.txt".to_string()));
        assert!(keys.contains(&"folder/subfolder/test3.txt".to_string()));

        // 清理
        client
            .delete_with_prefix("folder/")
            .await
            .expect("删除失败");
        assert_empty(&client, "测试结束后对象存储应为空").await;
    }

    #[serial]
    #[tokio::test]
    async fn test_download_range() {
        let client = OssClient::from_env();
        assert_empty(&client, "测试开始前对象存储应为空").await;
        let key = "test_range.txt";
        let content = "This is a test file for range download.";
        add_object(&client, key, content).await;

        // 下载部分内容
        let (mut download_stream, len) =
            client.download(key, Some((5, 15))).await.expect("下载失败");
        let mut downloaded_data = Vec::new();
        while let Some(chunk) = download_stream.try_next().await.expect("读取下载流失败") {
            downloaded_data.extend_from_slice(&chunk);
        }
        let downloaded_str = String::from_utf8(downloaded_data).expect("下载数据不是有效的UTF-8");
        assert_eq!(len, 11, "下载的内容长度应为请求的范围长度 (15 - 5 + 1)");
        assert_eq!(
            downloaded_str,
            &content[5..=15],
            "下载的范围数据应与原内容匹配"
        );

        // 清理
        client.delete(key).await.expect("删除失败");
        assert_empty(&client, "测试结束后对象存储应为空").await;
    }
}
