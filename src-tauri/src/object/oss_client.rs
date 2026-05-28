use crate::object::NextToken;
use crate::object::ObjectError;
use crate::stream::ByteStream;
use futures::stream::{self, StreamExt};
use futures_util::TryStreamExt;
use serde::Serialize;
use s3::Bucket;
use std::sync::{Arc, RwLock};
use tauri_plugin_log::log;

/// 包装 rust-s3 的 HeadObjectResult，保持外部 API 字段名不变
#[derive(Debug, Clone, Serialize)]
pub struct HeadObjectOutput {
    pub etag: Option<String>,
    pub content_length: Option<u64>,
}

/// 包装 rust-s3 的 Object，保持外部 API 字段名不变
#[derive(Debug, Clone, Serialize)]
pub struct Object {
    pub key: String,
    pub size: u64,
    pub etag: Option<String>,
}

struct OssClientInner {
    bucket: Box<Bucket>,
}

/// 从阿里云 OSS endpoint URL 中提取 region
/// 例: "https://oss-cn-guangzhou.aliyuncs.com" → "cn-guangzhou"
fn extract_region_from_endpoint(endpoint: &str) -> String {
    let host = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint)
        .split('/')
        .next()
        .unwrap_or(endpoint);
    // oss-cn-guangzhou.aliyuncs.com → cn-guangzhou
    // oss-cn-guangzhou-internal.aliyuncs.com → cn-guangzhou-internal
    if let Some(rest) = host.strip_prefix("oss-") {
        if let Some(region) = rest.strip_suffix(".aliyuncs.com") {
            return region.to_string();
        }
    }
    "cn-hangzhou".to_string()
}

#[derive(Clone)]
pub struct OssClient {
    inner: Arc<RwLock<Option<OssClientInner>>>,
}

impl OssClient {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    pub fn initialize(
        &self,
        endpoint: String,
        akid: String,
        sakey: String,
        bucket: String,
    ) -> Result<(), ObjectError> {
        // 先打日志再 trim，保留原始值用于诊断
        log::info!("[oss init] raw endpoint(len={}): {:?}", endpoint.len(), endpoint);
        log::info!("[oss init] raw bucket(len={}): {:?}", bucket.len(), bucket);
        log::info!("[oss init] raw akid(len={}): {:?}", akid.len(), akid);

        let endpoint = endpoint.trim().to_string();
        let akid = akid.trim().to_string();
        let sakey = sakey.trim().to_string();
        let bucket = bucket.trim().to_string();

        if endpoint.is_empty() || bucket.is_empty() || akid.is_empty() || sakey.is_empty() {
            return Err(ObjectError::CreateFailed(
                "OSS 配置不完整，请检查设置中是否已填写 AccessKey、Bucket 及 Endpoint".into()
            ));
        }

        let endpoint_url = if endpoint.starts_with("http") {
            endpoint
        } else {
            format!("https://{}", endpoint)
        };

        let credentials = s3::creds::Credentials::new(
            Some(&akid),
            Some(&sakey),
            None,
            None,
            None,
        )
        .map_err(|e| ObjectError::CreateFailed(e.to_string()))?;

        let region = s3::Region::Custom {
            region: extract_region_from_endpoint(&endpoint_url),
            endpoint: endpoint_url,
        };

        let bucket = Bucket::new(&bucket, region, credentials)
            .map_err(|e| ObjectError::CreateFailed(e.to_string()))?
            .with_service("oss");

        let mut guard = self.inner.write().map_err(|e| {
            ObjectError::OperationFailed(format!("Lock poisoned: {}", e))
        })?;
        *guard = Some(OssClientInner { bucket });
        Ok(())
    }

    fn inner(&self) -> Result<Box<Bucket>, ObjectError> {
        let guard = self.inner.read().map_err(|e| {
            ObjectError::OperationFailed(format!("Lock poisoned: {}", e))
        })?;
        let inner = guard.as_ref().ok_or(ObjectError::NotInitialized)?;
        Ok(inner.bucket.clone())
    }

    /// 重置客户端，清除内部状态（用于禁用远程存储时）
    pub fn reset(&self) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = None;
        }
    }

    #[cfg(test)]
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        let akid = std::env::var("ALIYUN_KEY").expect("请配置ALIYUN_KEY");
        let sakey = std::env::var("ALIYUN_SECRET").expect("请配置ALIYUN_SECRET");
        let bucket = std::env::var("ALIYUN_BUCKET_NAME").expect("请配置ALIYUN_BUCKET_NAME");
        let endpoint = std::env::var("ALIYUN_ENDPOINT").expect("请配置ALIYUN_ENDPOINT");
        let client = Self::new();
        client
            .initialize(endpoint, akid, sakey, bucket)
            .expect("创建 S3 client 失败");
        client
    }

    /// 重命名文件 (CopyObject + DeleteObject)
    pub async fn rename(&self, old_key: &str, new_key: &str) -> Result<(), ObjectError> {
        let bucket = self.inner()?;
        // 确保不存在新的键
        let (_, status) = bucket.head_object(new_key).await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        if (200..300).contains(&status) {
            return Err(ObjectError::KeyAlreadyExists(new_key.to_string()));
        }
        let copy_status = bucket.copy_object_internal(old_key, new_key).await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        if copy_status >= 300 {
            return Err(ObjectError::OperationFailed(format!("Copy failed: HTTP {}", copy_status)));
        }
        bucket.delete_object(old_key).await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(())
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/putobject
    pub async fn upload(
        &self,
        key: &str,
        _len: u64,
        stream: ByteStream,
        mimetype: &str,
    ) -> Result<String, ObjectError> {
        let bucket = self.inner()?;
        let mut reader = tokio_util::io::StreamReader::new(stream);
        bucket
            .put_object_stream_with_content_type(&mut reader, key, mimetype)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        // PutStreamResponse 不含 etag，通过 HEAD 获取
        let (result, _) = bucket.head_object(key).await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(result.e_tag.unwrap_or_default().trim_matches('"').to_string())
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/putobject
    pub async fn upload_bytes(&self, key: &str, data: &[u8]) -> Result<String, ObjectError> {
        let bucket = self.inner()?;
        let resp = bucket
            .put_object(key, data)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        let etag = resp.headers()
            .get("etag")
            .and_then(|v| v.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            .unwrap_or_default()
            .to_string();
        Ok(etag)
    }

    pub async fn delete(&self, key: &str) -> Result<(), ObjectError> {
        let bucket = self.inner()?;
        let resp = bucket
            .delete_object(key)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        if resp.status_code() >= 300 {
            return Err(ObjectError::OperationFailed(format!("Delete failed: HTTP {}", resp.status_code())));
        }
        Ok(())
    }

    async fn list_all_keys(&self, prefix: &str) -> Result<Vec<String>, ObjectError> {
        let mut keys = Vec::new();
        let mut token = None;
        loop {
            let (objs, next_token) = self.list(prefix, token).await?;
            keys.extend(objs.into_iter().map(|m| m.key));
            token = next_token;
            if token.is_none() {
                break;
            }
        }
        Ok(keys)
    }

    pub async fn delete_with_prefix(&self, prefix: &str) -> Result<Vec<String>, ObjectError> {
        let bucket = self.inner()?;
        let keys = self.list_all_keys(prefix).await?;
        if keys.is_empty() {
            return Ok(vec![]);
        }
        // 并发删除，最多 10 个并发
        let deleted_keys = keys.clone();
        let results: Vec<Result<_, _>> = stream::iter(keys)
            .map(|key| {
                let b = bucket.clone();
                async move {
                    b.delete_object(&key).await
                        .map_err(|e| ObjectError::OperationFailed(e.to_string()))
                }
            })
            .buffer_unordered(10)
            .collect()
            .await;
        for result in results {
            result?;
        }
        Ok(deleted_keys)
    }

    pub async fn get_metadata(&self, key: &str) -> Result<HeadObjectOutput, ObjectError> {
        let bucket = self.inner()?;
        let (result, status) = bucket
            .head_object(key)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        if status >= 400 {
            return Err(ObjectError::OperationFailed(format!("HEAD failed: HTTP {}", status)));
        }
        Ok(HeadObjectOutput {
            etag: result.e_tag.map(|e| e.trim_matches('"').to_string()),
            content_length: result.content_length.map(|v| v.max(0) as u64),
        })
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/listobjects-v2
    pub async fn list(
        &self,
        prefix: &str,
        next_token: NextToken,
    ) -> Result<(Vec<Object>, NextToken), ObjectError> {
        let bucket = self.inner()?;
        log::info!("[oss list] prefix={:?}, token={:?}", prefix, next_token);
        let max_keys: Option<usize> = if cfg!(debug_assertions) { Some(10) } else { None };
        let (result, _) = bucket
            .list_page(
                prefix.to_string(),
                None,
                next_token,
                None,
                max_keys,
            )
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;

        let next_token = result.next_continuation_token.clone();
        let objects: Vec<Object> = result
            .contents
            .into_iter()
            .map(|obj| Object {
                key: obj.key,
                size: obj.size,
                etag: obj.e_tag.map(|e| e.trim_matches('"').to_string()),
            })
            .collect();

        Ok((objects, next_token))
    }

    pub async fn download(
        &self,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<(ByteStream, u64), ObjectError> {
        let bucket = self.inner()?;
        if let Some((start, end)) = range {
            // Range 下载（buffered），end 为 inclusive
            let resp = bucket
                .get_object_range(key, start, Some(end))
                .await
                .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
            let content_len = end - start + 1;
            let bytes = resp.into_bytes();
            let s = stream::once(async move { Ok::<_, std::io::Error>(bytes) });
            Ok((Box::pin(s), content_len))
        } else {
            // 全量流式下载
            let resp_stream = bucket
                .get_object_stream(key)
                .await
                .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
            // 通过 HEAD 获取 content_length
            let (meta, _) = bucket.head_object(key).await
                .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
            let content_len = meta.content_length
                .ok_or_else(|| ObjectError::OperationFailed("missing content length".into()))?
                .max(0) as u64;
            let byte_stream = resp_stream.bytes
                .map(|r| r.map_err(|e| std::io::Error::other(e.to_string())));
            Ok((Box::pin(byte_stream), content_len))
        }
    }

    pub async fn download_bytes(&self, key: &str) -> Result<Vec<u8>, ObjectError> {
        let (mut stream, _) = self.download(key, None).await?;
        let mut data = Vec::new();
        while let Some(chunk) = stream.try_next().await.map_err(|e| ObjectError::OperationFailed(e.to_string()))? {
            data.extend_from_slice(&chunk);
        }
        Ok(data)
    }

    pub async fn direct_url(&self, key: &str) -> Result<String, ObjectError> {
        let bucket = self.inner()?;
        let url = bucket
            .presign_get(key, 3600, None)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(url)
    }

    /// 初始化分片上传，返回 upload_id
    pub async fn initiate_multipart_upload(
        &self,
        key: &str,
        content_type: &str,
    ) -> Result<String, ObjectError> {
        let bucket = self.inner()?;
        let resp = bucket
            .initiate_multipart_upload(key, content_type)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(resp.upload_id)
    }

    /// 上传单个分片，返回 (etag, part_number)
    pub async fn upload_part(
        &self,
        key: &str,
        part_number: u32,
        upload_id: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(String, u32), ObjectError> {
        let bucket = self.inner()?;
        let part = bucket
            .put_multipart_chunk(data, key, part_number, upload_id, content_type)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        let etag = part.etag.trim_matches('"').to_string();
        Ok((etag, part.part_number))
    }

    /// 完成分片上传，返回 composite ETag
    pub async fn complete_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
        parts: Vec<(String, u32)>,
    ) -> Result<String, ObjectError> {
        let bucket = self.inner()?;
        let s3_parts: Vec<s3::serde_types::Part> = parts
            .into_iter()
            .map(|(etag, part_number)| s3::serde_types::Part { etag, part_number })
            .collect();
        bucket
            .complete_multipart_upload(key, upload_id, s3_parts)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        // HEAD 获取 composite ETag（格式为 "hash-N"）
        let (result, status) = bucket.head_object(key).await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        log::info!("[oss] HEAD after complete: key={}, status={}, etag_raw={:?}, content_length={:?}", key, status, result.e_tag, result.content_length);
        Ok(result.e_tag.unwrap_or_default().trim_matches('"').to_string())
    }

    /// 取消分片上传
    pub async fn abort_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
    ) -> Result<(), ObjectError> {
        let bucket = self.inner()?;
        bucket
            .abort_upload(key, upload_id)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(())
    }
}
