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
        // 直接调用，暂时不处理保存到本地，在download再处理
        self.oss.upload(key, len, stream, mimetype).await
    }

    pub async fn delete(&self, key: &str) -> Result<(), String> {
        self.oss.delete(key).await?;
        // 更新内存map
        self.keys_etag_map.remove(key);
        // 删除本地缓存
        self.cache.delete(key).await;
        Ok(())
    }

    pub async fn delete_with_prefix(&self, prefix: &str) -> Result<Vec<String>, String> {
        let del_keys = self.oss.delete_with_prefix(prefix).await?;
        for del_key in &del_keys {
            self.cache.delete(del_key).await;
            self.keys_etag_map.remove(del_key);
        }
        Ok(del_keys)
    }

    pub async fn get_metadata(&self, key: &str) -> Result<ObjectMetadata, String> {
        let metadata = self.oss.get_metadata(key).await?;
        if let Some(res) = self.keys_etag_map.get(&key.to_string()) {
            let key = res.key();
            let (md5_str, size) = res.value();
            if *size != metadata.size() || *md5_str != metadata.etag() {
                self.keys_etag_map
                    .insert(key.to_string(), (md5_str.to_string(), metadata.size()));
            }
        } else {
            self.keys_etag_map.insert(
                key.to_string(),
                (metadata.etag().to_string(), metadata.size()),
            );
        }
        Ok(metadata)
    }

    pub async fn download(
        &self,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<(ByteStream, u64), String> {
        if range.is_none() {
            // 全量下下载时考虑使用缓存
        }
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
