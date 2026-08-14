use crate::object::NextToken;
use crate::object::ObjectError;
use crate::stream::ByteStream;
use futures::stream::{self, StreamExt};
use futures_util::TryStreamExt;
use s3::Bucket;
use serde::Serialize;
use std::sync::{Arc, RwLock};
use tauri_plugin_log::log;
use tokio::io::AsyncReadExt;
use tokio_util::io::StreamReader;

const STREAM_UPLOAD_CHUNK_SIZE: usize = 8 * 1024 * 1024;
const COMMON_PREFIX_PAGE_SIZE: usize = if cfg!(debug_assertions) { 10 } else { 50 };

fn oss_config_diagnostics(endpoint: &str, akid: &str, sakey: &str, bucket: &str) -> String {
    format!(
        "[oss init] config lengths: endpoint={}, bucket={}, akid={}, sakey={}",
        endpoint.len(),
        bucket.len(),
        akid.len(),
        sakey.len()
    )
}

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
    key_prefix: String,
}

impl OssClient {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            key_prefix: String::new(),
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    #[cfg(test)]
    pub fn with_key_prefix(&self, prefix: String) -> Self {
        let key_prefix = prefix.trim_matches('/');
        Self {
            inner: self.inner.clone(),
            key_prefix: if key_prefix.is_empty() {
                String::new()
            } else {
                format!("{key_prefix}/")
            },
        }
    }

    fn physical_key(&self, key: &str) -> String {
        format!("{}{}", self.key_prefix, key.trim_start_matches('/'))
    }

    fn logical_key(&self, key: String) -> String {
        key.strip_prefix(&self.key_prefix)
            .unwrap_or(&key)
            .to_string()
    }

    pub fn initialize(
        &self,
        endpoint: String,
        akid: String,
        sakey: String,
        bucket: String,
    ) -> Result<(), ObjectError> {
        log::info!(
            "{}",
            oss_config_diagnostics(&endpoint, &akid, &sakey, &bucket)
        );

        let endpoint = endpoint.trim().to_string();
        let akid = akid.trim().to_string();
        let sakey = sakey.trim().to_string();
        let bucket = bucket.trim().to_string();

        if endpoint.is_empty() || bucket.is_empty() || akid.is_empty() || sakey.is_empty() {
            return Err(ObjectError::CreateFailed(
                "OSS 配置不完整，请检查设置中是否已填写 AccessKey、Bucket 及 Endpoint".into(),
            ));
        }

        let endpoint_url = if endpoint.starts_with("http") {
            endpoint
        } else {
            format!("https://{}", endpoint)
        };

        let credentials = s3::creds::Credentials::new(Some(&akid), Some(&sakey), None, None, None)
            .map_err(|e| ObjectError::CreateFailed(e.to_string()))?;

        let region = s3::Region::Custom {
            region: extract_region_from_endpoint(&endpoint_url),
            endpoint: endpoint_url,
        };

        let bucket = Bucket::new(&bucket, region, credentials)
            .map_err(|e| ObjectError::CreateFailed(e.to_string()))?
            .with_service("oss");

        let mut guard = self
            .inner
            .write()
            .map_err(|e| ObjectError::OperationFailed(format!("Lock poisoned: {}", e)))?;
        *guard = Some(OssClientInner { bucket });
        Ok(())
    }

    fn inner(&self) -> Result<Box<Bucket>, ObjectError> {
        let guard = self
            .inner
            .read()
            .map_err(|e| ObjectError::OperationFailed(format!("Lock poisoned: {}", e)))?;
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

    /// https://help.aliyun.com/zh/oss/developer-reference/putobject
    pub async fn upload(
        &self,
        key: &str,
        len: u64,
        stream: ByteStream,
        mimetype: &str,
    ) -> Result<String, ObjectError> {
        log::info!(
            "[oss] stream upload started: key={}, size={}, content_type={}",
            key,
            len,
            mimetype
        );
        let mut reader = StreamReader::new(stream);

        // 小对象仍使用普通 PutObject，内存上限不超过一个分片。
        if len <= STREAM_UPLOAD_CHUNK_SIZE as u64 {
            let mut data = Vec::with_capacity(len as usize);
            reader.read_to_end(&mut data).await.map_err(|error| {
                ObjectError::OperationFailed(format!("读取上传流失败：{error}"))
            })?;
            if data.len() as u64 != len {
                return Err(ObjectError::OperationFailed(format!(
                    "上传流大小不匹配：expected={len}, actual={}",
                    data.len()
                )));
            }
            let etag = self.upload_bytes(key, &data).await?;
            log::info!(
                "[oss] stream upload completed: key={}, size={}, etag={}",
                self.physical_key(key),
                len,
                etag
            );
            return Ok(etag);
        }

        // 大对象显式串行 multipart。rust-s3 的流接口会根据可用内存同时保留
        // 最多 100 个 8MB 分片，并在请求构建时再次复制每个分片。
        let upload_id = self.initiate_multipart_upload(key, mimetype).await?;
        let mut parts = Vec::new();
        let mut transferred = 0u64;
        let mut part_number = 1u32;

        loop {
            let mut chunk = Vec::with_capacity(STREAM_UPLOAD_CHUNK_SIZE);
            while chunk.len() < STREAM_UPLOAD_CHUNK_SIZE {
                match reader.read_buf(&mut chunk).await {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) => {
                        let primary =
                            ObjectError::OperationFailed(format!("读取上传流失败：{error}"));
                        return Err(self
                            .abort_upload_after_error(key, &upload_id, primary)
                            .await);
                    }
                }
            }
            if chunk.is_empty() {
                break;
            }

            transferred = transferred.saturating_add(chunk.len() as u64);
            if transferred > len {
                let primary = ObjectError::OperationFailed(format!(
                    "上传流超过声明大小：expected={len}, actual>{len}"
                ));
                return Err(self
                    .abort_upload_after_error(key, &upload_id, primary)
                    .await);
            }

            match self
                .upload_part(key, part_number, &upload_id, chunk, mimetype)
                .await
            {
                Ok((etag, returned_part_number)) if returned_part_number == part_number => {
                    parts.push((etag, part_number));
                }
                Ok((_, returned_part_number)) => {
                    let primary = ObjectError::OperationFailed(format!(
                        "对象存储返回了错误的分片编号：expected={part_number}, actual={returned_part_number}"
                    ));
                    return Err(self
                        .abort_upload_after_error(key, &upload_id, primary)
                        .await);
                }
                Err(primary) => {
                    return Err(self
                        .abort_upload_after_error(key, &upload_id, primary)
                        .await);
                }
            }
            part_number += 1;
        }

        if transferred != len {
            let primary = ObjectError::OperationFailed(format!(
                "上传流大小不匹配：expected={len}, actual={transferred}"
            ));
            return Err(self
                .abort_upload_after_error(key, &upload_id, primary)
                .await);
        }

        let etag = match self.complete_multipart_upload(key, &upload_id, parts).await {
            Ok(etag) => etag,
            Err(primary) => {
                return Err(self
                    .abort_upload_after_error(key, &upload_id, primary)
                    .await);
            }
        };
        log::info!(
            "[oss] stream upload completed: key={}, size={}, etag={}",
            self.physical_key(key),
            len,
            etag
        );
        Ok(etag)
    }

    async fn abort_upload_after_error(
        &self,
        key: &str,
        upload_id: &str,
        primary: ObjectError,
    ) -> ObjectError {
        match self.abort_multipart_upload(key, upload_id).await {
            Ok(()) => primary,
            Err(abort_error) => ObjectError::OperationFailed(format!(
                "{primary}；取消 multipart 失败：{abort_error}"
            )),
        }
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/putobject
    pub async fn upload_bytes(&self, key: &str, data: &[u8]) -> Result<String, ObjectError> {
        let bucket = self.inner()?;
        let key = self.physical_key(key);
        let resp = bucket
            .put_object(&key, data)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            .unwrap_or_default()
            .to_string();
        Ok(etag)
    }

    pub async fn delete(&self, key: &str) -> Result<(), ObjectError> {
        let bucket = self.inner()?;
        let key = self.physical_key(key);
        let resp = bucket
            .delete_object(&key)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        if resp.status_code() >= 300 {
            return Err(ObjectError::OperationFailed(format!(
                "Delete failed: HTTP {}",
                resp.status_code()
            )));
        }
        Ok(())
    }

    pub(crate) async fn list_all_keys(&self, prefix: &str) -> Result<Vec<String>, ObjectError> {
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

    pub(crate) async fn delete_keys(&self, keys: Vec<String>) -> Result<Vec<String>, ObjectError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        // 尽量完成同批次中的所有删除，并汇总失败；调用方可以据此决定是否
        // 继续删除作为提交标志的 manifest。
        let results: Vec<(String, Result<(), ObjectError>)> = stream::iter(keys)
            .map(|key| {
                let client = self.clone();
                async move {
                    let result = client.delete(&key).await;
                    (key, result)
                }
            })
            .buffer_unordered(10)
            .collect()
            .await;

        let mut deleted_keys = Vec::new();
        let mut failures = Vec::new();
        for (key, result) in results {
            match result {
                Ok(()) => deleted_keys.push(key),
                Err(error) => failures.push(format!("{key}: {error}")),
            }
        }

        if failures.is_empty() {
            Ok(deleted_keys)
        } else {
            Err(ObjectError::OperationFailed(format!(
                "failed to delete {} object(s): {}",
                failures.len(),
                failures.join("; ")
            )))
        }
    }

    #[cfg(test)]
    pub async fn delete_with_prefix(&self, prefix: &str) -> Result<Vec<String>, ObjectError> {
        let keys = self.list_all_keys(prefix).await?;
        self.delete_keys(keys).await
    }

    pub async fn get_metadata(&self, key: &str) -> Result<HeadObjectOutput, ObjectError> {
        let bucket = self.inner()?;
        let key = self.physical_key(key);
        let (result, status) = bucket
            .head_object(&key)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        if status >= 400 {
            return Err(ObjectError::OperationFailed(format!(
                "HEAD failed: HTTP {}",
                status
            )));
        }
        Ok(HeadObjectOutput {
            etag: result.e_tag.map(|e| e.trim_matches('"').to_string()),
            content_length: result.content_length.map(|v| v.max(0) as u64),
        })
    }

    /// 判断对象是否存在；只有明确的 404 会返回 false，其他异常仍向上传递。
    pub async fn object_exists(&self, key: &str) -> Result<bool, ObjectError> {
        let bucket = self.inner()?;
        let key = self.physical_key(key);
        let (_, status) = bucket
            .head_object(&key)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        match status {
            200..=299 => Ok(true),
            404 => Ok(false),
            _ => Err(ObjectError::OperationFailed(format!(
                "HEAD failed: HTTP {status}"
            ))),
        }
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/listobjects-v2
    pub async fn list(
        &self,
        prefix: &str,
        next_token: NextToken,
    ) -> Result<(Vec<Object>, NextToken), ObjectError> {
        let bucket = self.inner()?;
        let physical_prefix = self.physical_key(prefix);
        log::info!(
            "[oss list] prefix={:?}, token={:?}",
            physical_prefix,
            next_token
        );
        let max_keys: Option<usize> = if cfg!(debug_assertions) {
            Some(10)
        } else {
            None
        };
        let (result, _) = bucket
            .list_page(physical_prefix, None, next_token, None, max_keys)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;

        let next_token = result.next_continuation_token.clone();
        let objects: Vec<Object> = result
            .contents
            .into_iter()
            .map(|obj| Object {
                key: self.logical_key(obj.key),
                size: obj.size,
                etag: obj.e_tag.map(|e| e.trim_matches('"').to_string()),
            })
            .collect();

        Ok((objects, next_token))
    }

    /// 使用 `/` 分隔符列出指定前缀下的直接子目录，不展开目录内对象。
    pub async fn list_common_prefixes(
        &self,
        prefix: &str,
        next_token: NextToken,
    ) -> Result<(Vec<String>, NextToken), ObjectError> {
        let bucket = self.inner()?;
        let physical_prefix = self.physical_key(prefix);
        log::info!(
            "[oss list prefixes] prefix={:?}, token={:?}",
            physical_prefix,
            next_token
        );
        let (result, _) = bucket
            .list_page(
                physical_prefix,
                Some("/".to_string()),
                next_token,
                None,
                Some(COMMON_PREFIX_PAGE_SIZE),
            )
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;

        let next_token = result.next_continuation_token.clone();
        let prefixes = result
            .common_prefixes
            .unwrap_or_default()
            .into_iter()
            .map(|prefix| self.logical_key(prefix.prefix))
            .collect();
        Ok((prefixes, next_token))
    }

    pub async fn download(
        &self,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<(ByteStream, u64), ObjectError> {
        let bucket = self.inner()?;
        let key = self.physical_key(key);
        if let Some((start, end)) = range {
            // Range 下载（buffered），end 为 inclusive
            let resp = bucket
                .get_object_range(&key, start, Some(end))
                .await
                .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
            let content_len = end - start + 1;
            let bytes = resp.into_bytes();
            let s = stream::once(async move { Ok::<_, std::io::Error>(bytes) });
            Ok((Box::pin(s), content_len))
        } else {
            // 全量流式下载
            let resp_stream = bucket
                .get_object_stream(&key)
                .await
                .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
            // 通过 HEAD 获取 content_length
            let (meta, _) = bucket
                .head_object(&key)
                .await
                .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
            let content_len = meta
                .content_length
                .ok_or_else(|| ObjectError::OperationFailed("missing content length".into()))?
                .max(0) as u64;
            let byte_stream = resp_stream
                .bytes
                .map(|r| r.map_err(|e| std::io::Error::other(e.to_string())));
            Ok((Box::pin(byte_stream), content_len))
        }
    }

    pub async fn download_bytes(&self, key: &str) -> Result<Vec<u8>, ObjectError> {
        let (mut stream, _) = self.download(key, None).await?;
        let mut data = Vec::new();
        while let Some(chunk) = stream
            .try_next()
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?
        {
            data.extend_from_slice(&chunk);
        }
        Ok(data)
    }

    #[cfg(test)]
    pub async fn direct_url(&self, key: &str) -> Result<String, ObjectError> {
        let bucket = self.inner()?;
        let key = self.physical_key(key);
        let url = bucket
            .presign_get(&key, 3600, None)
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
        let key = self.physical_key(key);
        let resp = bucket
            .initiate_multipart_upload(&key, content_type)
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
        let key = self.physical_key(key);
        let part = bucket
            .put_multipart_chunk(data, &key, part_number, upload_id, content_type)
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
        let key = self.physical_key(key);
        let s3_parts: Vec<s3::serde_types::Part> = parts
            .into_iter()
            .map(|(etag, part_number)| s3::serde_types::Part { etag, part_number })
            .collect();
        bucket
            .complete_multipart_upload(&key, upload_id, s3_parts)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        // HEAD 获取 composite ETag（格式为 "hash-N"）
        let (result, status) = bucket
            .head_object(&key)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        log::info!(
            "[oss] HEAD after complete: key={}, status={}, etag_raw={:?}, content_length={:?}",
            key,
            status,
            result.e_tag,
            result.content_length
        );
        Ok(result
            .e_tag
            .unwrap_or_default()
            .trim_matches('"')
            .to_string())
    }

    /// 取消分片上传
    pub async fn abort_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
    ) -> Result<(), ObjectError> {
        let bucket = self.inner()?;
        let key = self.physical_key(key);
        bucket
            .abort_upload(&key, upload_id)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(())
    }

    /// 测试中直接查询当前唯一前缀下尚未完成的 multipart 会话。
    #[cfg(test)]
    pub async fn list_multipart_uploads(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, String)>, ObjectError> {
        let bucket = self.inner()?;
        let prefix = self.physical_key(prefix);
        let pages = bucket
            .list_multiparts_uploads(Some(&prefix), None)
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(pages
            .into_iter()
            .flat_map(|page| page.uploads)
            .map(|upload| (self.logical_key(upload.key), upload.id))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::oss_config_diagnostics;

    #[test]
    fn oss_config_diagnostics_never_contains_configuration_values() {
        let endpoint = "secret-endpoint.example";
        let akid = "sentinel-access-key";
        let sakey = "sentinel-access-secret";
        let bucket = "sentinel-bucket";

        let diagnostics = oss_config_diagnostics(endpoint, akid, sakey, bucket);

        for value in [endpoint, akid, sakey, bucket] {
            assert!(!diagnostics.contains(value));
        }
        assert!(diagnostics.contains(&format!("endpoint={}", endpoint.len())));
        assert!(diagnostics.contains(&format!("bucket={}", bucket.len())));
        assert!(diagnostics.contains(&format!("akid={}", akid.len())));
        assert!(diagnostics.contains(&format!("sakey={}", sakey.len())));
    }
}
