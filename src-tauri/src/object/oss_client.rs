use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::Utc;
use futures_util::TryStreamExt;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use std::io::Error;
use std::sync::Arc;

use crate::object::object_types::{
    AliyunListObjectsResult, AliyunObjectSummary, ObjectMetadata,
    ATTACHMENT_URL_EXPIRATION_SECONDS, STREAM_MINE_TYPE,
};
use crate::object::NextToken;
use crate::stream::ByteStream;
use tauri::http::header::{CONTENT_TYPE, DATE};
use tauri::http::{HeaderMap, HeaderValue, Method};

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

    /// https://help.aliyun.com/zh/oss/developer-reference/putobject
    pub async fn upload(
        &self,
        key: &str,
        len: u64,
        stream: ByteStream,
        mimetype: &str,
    ) -> Result<String, String> {
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
            let etag = resp
                .headers()
                .get("Etag")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.replace("\"", ""))
                .unwrap_or_default();
            Ok(etag)
        } else {
            Err(format!("上传失败 状态码:{}", resp.status()))
        }
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/putobject
    pub async fn upload_bytes(&self, key: &str, data: &[u8]) -> Result<String, String> {
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
            .body(data.to_owned())
            .send()
            .await
            .map_err(|e| format!("请求失败:{}", e))?;

        if resp.status().is_success() {
            let etag = resp
                .headers()
                .get("ETag")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.replace("\"", ""))
                .unwrap_or_default();
            Ok(etag)
        } else {
            Err(format!("上传字节失败 状态码:{}", resp.status()))
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

    pub async fn delete_with_prefix(&self, prefix: &str) -> Result<Vec<String>, String> {
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
        // 逐个删除对象
        for key in &needs_deletion {
            self.delete(key).await?;
        }
        Ok(needs_deletion)
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

        Ok(ObjectMetadata::new(
            key.to_string(),
            size,
            last_modified,
            etag,
        ))
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

        let stream = resp.bytes_stream().map_err(Error::other);

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

    /// 生成预签名 URL（Direct URL），允许外部直接访问私有对象
    pub fn direct_url(&self, key: &str) -> Result<String, String> {
        // 计算过期时间（当前时间 + 有效秒数）
        let expires = Utc::now().timestamp() + ATTACHMENT_URL_EXPIRATION_SECONDS;

        // 构造签名字符串 (CanonicalizedResource)
        // 格式：VERB + \n + Content-MD5 + \n + Content-Type + \n + Expires + \n + CanonicalizedResource
        // 对于 URL 签名，Content-MD5 和 Content-Type 通常为空
        let canonicalized_resource = format!("/{}/{}", self.inner.bucket, key);
        let string_to_sign = format!(
            "{}\n\n\n{}\n{}",
            Method::GET.as_str(),
            expires,
            canonicalized_resource
        );

        // HMAC-SHA1 签名
        type HmacSha1 = Hmac<Sha1>;
        let mut mac = HmacSha1::new_from_slice(self.inner.sakey.as_bytes())
            .map_err(|e| format!("Key 长度非法: {}", e))?;
        mac.update(string_to_sign.as_bytes());

        let signature = STANDARD.encode(mac.finalize().into_bytes());

        // 对签名进行 URL 编码（防止特殊字符破坏 URL 结构）
        // 注意：这里需要对 signature 进行 url encode，虽然 base64 只有 + / =，但仍建议处理
        let encoded_signature = urlencoding::encode(&signature);

        // 拼接最终 URL
        // 格式：https://<bucket>.<endpoint>/<key>?OSSAccessKeyId=<ak>&Expires=<expires>&Signature=<sig>
        let url = format!(
            "https://{}.{}/{}?OSSAccessKeyId={}&Expires={}&Signature={}",
            self.inner.bucket,
            self.inner.endpoint,
            key,
            self.inner.akid,
            expires,
            encoded_signature
        );

        Ok(url)
    }
}
