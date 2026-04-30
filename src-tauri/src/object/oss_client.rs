use crate::object::NextToken;
use crate::object::ObjectError;
use crate::stream::ByteStream;
use bytes::Bytes;
use futures::StreamExt;
use futures_util::TryStreamExt;
use s3::types::{HeadObjectOutput, Object};
use s3::{Auth, Client, Credentials};
use std::sync::{Arc, RwLock};

/// 将项目的 ByteStream（无 Sync）适配为 s3 crate 上传要求的格式（需要 Sync）
fn to_s3_upload_stream(
    stream: ByteStream,
) -> impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(8);
    tokio::spawn(async move {
        futures::pin_mut!(stream);
        while let Some(item) = stream.next().await {
            if tx.send(item).await.is_err() {
                break;
            }
        }
    });
    tokio_stream::wrappers::ReceiverStream::new(rx)
}

struct OssClientInner {
    client: Client,
    bucket: String,
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
        let endpoint_url = if endpoint.starts_with("http") {
            endpoint
        } else {
            format!("https://{}", endpoint)
        };

        let auth = Auth::Static(Credentials {
            access_key_id: akid,
            secret_access_key: sakey,
            session_token: None,
        });

        let client = Client::builder(&endpoint_url)
            .map_err(|e| ObjectError::CreateFailed(e.to_string()))?
            .region("oss-cn-hangzhou")
            .auth(auth)
            .build()
            .map_err(|e| ObjectError::CreateFailed(e.to_string()))?;

        let mut guard = self.inner.write().map_err(|e| {
            ObjectError::OperationFailed(format!("Lock poisoned: {}", e))
        })?;
        *guard = Some(OssClientInner { client, bucket });
        Ok(())
    }

    fn inner(&self) -> Result<(Client, String), ObjectError> {
        let guard = self.inner.read().map_err(|e| {
            ObjectError::OperationFailed(format!("Lock poisoned: {}", e))
        })?;
        let inner = guard.as_ref().ok_or(ObjectError::NotInitialized)?;
        Ok((inner.client.clone(), inner.bucket.clone()))
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
        let (client, bucket) = self.inner()?;
        // 确保不存在新的键
        let res = client.objects().head(&bucket, new_key).send().await;
        if let Ok(_res) = res {
            return Err(ObjectError::KeyAlreadyExists(new_key.to_string()));
        }
        client
            .objects()
            .copy(&bucket, old_key, &bucket, new_key)
            .send()
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        client
            .objects()
            .delete(&bucket, old_key)
            .send()
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(())
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/putobject
    pub async fn upload(
        &self,
        key: &str,
        len: u64,
        stream: ByteStream,
        mimetype: &str,
    ) -> Result<String, ObjectError> {
        let (client, bucket) = self.inner()?;
        let s3_stream = to_s3_upload_stream(stream);
        let resp = client
            .objects()
            .put(&bucket, key)
            .body_stream_sized(s3_stream, len)
            .content_type(mimetype)
            .send()
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(resp.etag.unwrap_or_default().trim_matches('"').to_string())
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/putobject
    pub async fn upload_bytes(&self, key: &str, data: &[u8]) -> Result<String, ObjectError> {
        let (client, bucket) = self.inner()?;
        let resp = client
            .objects()
            .put(&bucket, key)
            .body_bytes(data.to_vec())
            .content_length(data.len() as u64)
            .send()
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(resp.etag.unwrap_or_default().trim_matches('"').to_string())
    }

    pub async fn delete(&self, key: &str) -> Result<(), ObjectError> {
        let (client, bucket) = self.inner()?;
        client
            .objects()
            .delete(&bucket, key)
            .send()
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
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
        let (client, bucket) = self.inner()?;
        let keys = self.list_all_keys(prefix).await?;
        if keys.is_empty() {
            return Ok(vec![]);
        }
        client
            .objects()
            .delete_objects(&bucket)
            .objects(keys.clone())
            .send()
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        Ok(keys)
    }

    pub async fn get_metadata(&self, key: &str) -> Result<HeadObjectOutput, ObjectError> {
        let (client, bucket) = self.inner()?;
        let mut resp = client
            .objects()
            .head(&bucket, key)
            .send()
            .await
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        resp.etag = resp.etag.map(|e| e.trim_matches('"').to_string());
        Ok(resp)
    }

    /// https://help.aliyun.com/zh/oss/developer-reference/listobjects-v2
    pub async fn list(
        &self,
        prefix: &str,
        next_token: NextToken,
    ) -> Result<(Vec<Object>, NextToken), ObjectError> {
        let (client, bucket) = self.inner()?;
        let mut req = client.objects().list_v2(&bucket).prefix(prefix);
        if let Some(token) = next_token {
            req = req.continuation_token(token);
        }
        #[cfg(debug_assertions)]
        {
            req = req.max_keys(10);
        }
        let mut resp = req.send().await.map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        for obj in &mut resp.contents {
            if let Some(ref mut etag) = obj.etag {
                *etag = etag.trim_matches('"').to_string();
            }
        }
        Ok((resp.contents, resp.next_continuation_token))
    }

    pub async fn download(
        &self,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<(ByteStream, u64), ObjectError> {
        let (client, bucket) = self.inner()?;
        let mut req = client.objects().get(&bucket, key);
        if let Some((start, end)) = range {
            req = req.range_bytes(start, end);
        }
        let resp = req.send().await.map_err(|e| ObjectError::OperationFailed(e.to_string()))?;
        let content_len = resp.content_length;
        let stream = resp.body.map_err(|e| {
            std::io::Error::other(e.to_string())
        });
        let content_len = content_len
            .ok_or_else(|| ObjectError::OperationFailed("missing content length".into()))?;
        Ok((Box::pin(stream), content_len))
    }

    pub async fn download_bytes(&self, key: &str) -> Result<Vec<u8>, ObjectError> {
        let (mut stream, _) = self.download(key, None).await?;
        let mut data = Vec::new();
        while let Some(chunk) = stream.try_next().await.map_err(|e| ObjectError::OperationFailed(e.to_string()))? {
            data.extend_from_slice(&chunk);
        }
        Ok(data)
    }

    pub fn direct_url(&self, key: &str) -> Result<String, ObjectError> {
        let (client, bucket) = self.inner()?;
        let url = client
            .objects()
            .presign_get(&bucket, key)
            .expires_in(std::time::Duration::from_secs(3600))
            .build()
            .map_err(|e| ObjectError::OperationFailed(e.to_string()))?
            .url
            .to_string();
        Ok(url)
    }
}
