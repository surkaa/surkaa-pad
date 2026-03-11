use crate::object::{ByteStream, NextToken, ObjectMetadata, OssClient};
use crate::utils::LocalCache;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct CacheOssClient {
    oss: OssClient,
    cache: LocalCache,
    /// key: object key, value: object (etag, size)
    keys_etag_map: Arc<DashMap<String, (String, u64)>>,
}

impl CacheOssClient {
    pub fn new(oss: OssClient, cache: LocalCache) -> Self {
        Self {
            oss,
            cache,
            keys_etag_map: Arc::new(DashMap::new()),
        }
    }

    /// 参考 [`OssClient`] 的 `list` 方法
    pub async fn list(
        &self,
        prefix: &str,
        next_token: NextToken,
    ) -> Result<(Vec<ObjectMetadata>, NextToken), String> {
        let (objects, nt) = self.oss.list(prefix, next_token).await?;
        for object in &objects {
            let key = object.key().to_string();
            let etag = object.etag().to_string();
            let size = object.size();
            self.keys_etag_map.insert(key, (etag, size));
        }
        Ok((objects, nt))
    }

    pub async fn upload_bytes(&self, key: &str, data: &[u8]) -> Result<(), String> {
        self.oss.upload_bytes(key, data).await?;
        // 更新本地缓存
        let etag = self.cache.save_bytes(key, data).await?;
        // 更新内存map
        let size = data.len() as u64;
        self.keys_etag_map.insert(key.to_string(), (etag, size));
        Ok(())
    }

    /// 参考 [`OssClient`] 的 `upload` 方法
    pub async fn upload(
        &self,
        key: &str,
        len: u64,
        stream: ByteStream,
        mimetype: &str,
    ) -> Result<(), String> {
        self.oss.upload(key, len, stream, mimetype).await?;
        // 更新本地缓存
        let etag = self.cache.save(key, stream).await?;
        // 更新内存map
        self.keys_etag_map.insert(key.to_string(), (etag, len));
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), String> {
        todo!()
    }

    pub async fn delete_with_prefix(&self, prefix: &str) -> Result<u32, String> {
        todo!()
    }

    pub async fn get_metadata(&self, key: &str) -> Result<ObjectMetadata, String> {
        self.oss.get_metadata(key).await
    }

    pub async fn download(
        &self,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<(ByteStream, u64), String> {
        todo!()
    }

    pub async fn download_bytes(&self, key: &str) -> Result<Vec<u8>, String> {
        todo!()
    }

    /// 生成预签名 URL（Direct URL），允许外部直接访问私有对象
    pub fn direct_url(&self, key: &str) -> Result<String, String> {
        self.oss.direct_url(key)
    }
}
