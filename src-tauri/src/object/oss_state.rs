use crate::object::{ByteStream, NextToken, ObjectMetadata, OssClient};
use bytes::Bytes;
use futures_util::stream;
use futures_util::TryStreamExt;
use std::collections::HashMap;
use std::io::Error;
use std::path::{Component, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// 将 `key` 拼接 `suffix` 后与 `base` 合并，生成本地缓存路径。
/// 若 `key` 中存在路径穿越分量（`..`）或绝对路径前缀，返回错误。
fn safe_cache_path(base: &PathBuf, key: &str, suffix: &str) -> Result<PathBuf, String> {
    let relative = PathBuf::from(format!("{}{}", key, suffix));
    for component in relative.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("非法的对象键，含路径穿越: {}", key));
            }
            _ => {}
        }
    }
    Ok(base.join(relative))
}

struct OssStateInner {
    client: OnceLock<OssClient>,
    cache_dir: PathBuf,
    etag_cache: Mutex<HashMap<String, String>>,
}

#[derive(Clone)]
pub struct OssState(Arc<OssStateInner>);

impl OssState {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self(Arc::new(OssStateInner {
            client: OnceLock::new(),
            cache_dir,
            etag_cache: Mutex::new(HashMap::new()),
        }))
    }

    pub async fn initialize(
        &self,
        akid: String,
        sakey: String,
        endpoint: String,
        bucket: String,
    ) -> Result<(), String> {
        // 创建 OssClient
        let client = OssClient::new(endpoint, akid, sakey, bucket);
        // 测试 client 是否可用
        let _ = client.list("", None).await?;
        // 存储 client
        self.0
            .client
            .set(client)
            .map_err(|_| String::from("OssClient 已初始化"))?;
        Ok(())
    }

    pub fn get_client(&self) -> Result<OssClient, String> {
        self.0
            .client
            .get()
            .cloned()
            .ok_or(String::from("OssClient 未初始化"))
    }

    /// 获取缓存数据文件路径：<cache_dir>/<key>.dat
    /// 若 key 含路径穿越分量（如 `..`）或绝对路径，返回错误
    fn cache_data_path(&self, key: &str) -> Result<PathBuf, String> {
        safe_cache_path(&self.0.cache_dir, key, ".dat")
    }

    /// 获取缓存 ETag 文件路径：<cache_dir>/<key>.etag
    /// 若 key 含路径穿越分量（如 `..`）或绝对路径，返回错误
    fn cache_etag_path(&self, key: &str) -> Result<PathBuf, String> {
        safe_cache_path(&self.0.cache_dir, key, ".etag")
    }

    /// 确保内存 ETag 缓存已加载；若为空则通过 OssClient.list() 拉取所有对象的 ETag
    async fn ensure_etag_cache(&self) -> Result<(), String> {
        let is_empty = {
            let cache = self
                .0
                .etag_cache
                .lock()
                .map_err(|_| "锁定ETag缓存失败".to_string())?;
            cache.is_empty()
        };
        if is_empty {
            let client = self.get_client()?;
            let mut next_token = None;
            let mut new_cache = HashMap::new();
            loop {
                let (objects, nt) = client.list("", next_token).await?;
                for obj in objects {
                    new_cache.insert(obj.key().to_string(), obj.etag().to_string());
                }
                if nt.is_none() {
                    break;
                }
                next_token = nt;
            }
            let mut cache = self
                .0
                .etag_cache
                .lock()
                .map_err(|_| "锁定ETag缓存失败".to_string())?;
            *cache = new_cache;
        }
        Ok(())
    }

    /// 在内存 ETag 缓存中插入或更新一条记录
    fn update_etag_cache(&self, key: &str, etag: &str) -> Result<(), String> {
        let mut cache = self
            .0
            .etag_cache
            .lock()
            .map_err(|_| "锁定ETag缓存失败".to_string())?;
        cache.insert(key.to_string(), etag.to_string());
        Ok(())
    }

    /// 从内存 ETag 缓存中移除一条记录
    fn remove_from_etag_cache(&self, key: &str) -> Result<(), String> {
        let mut cache = self
            .0
            .etag_cache
            .lock()
            .map_err(|_| "锁定ETag缓存失败".to_string())?;
        cache.remove(key);
        Ok(())
    }

    /// 将数据及其 ETag 写入本地缓存文件
    fn write_local_cache(&self, key: &str, data: &[u8], etag: &str) -> Result<(), String> {
        let data_path = self.cache_data_path(key)?;
        let etag_path = self.cache_etag_path(key)?;
        if let Some(parent) = data_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建缓存目录失败: {}", e))?;
        }
        std::fs::write(&data_path, data).map_err(|e| format!("写入缓存数据失败: {}", e))?;
        std::fs::write(&etag_path, etag).map_err(|e| format!("写入缓存ETag失败: {}", e))?;
        Ok(())
    }

    /// 删除本地缓存中的 .dat 和 .etag 文件（忽略不存在的情况）
    fn delete_local_cache(&self, key: &str) {
        if let Ok(path) = self.cache_data_path(key) {
            let _ = std::fs::remove_file(path);
        }
        if let Ok(path) = self.cache_etag_path(key) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// 上传流数据；上传成功后通过 get_metadata 更新内存 ETag 缓存
    pub async fn upload(
        &self,
        key: &str,
        len: u64,
        stream: ByteStream,
        mimetype: &str,
    ) -> Result<(), String> {
        let client = self.get_client()?;
        client.upload(key, len, stream, mimetype).await?;
        let metadata = client.get_metadata(key).await?;
        let etag = metadata.etag().to_string();
        self.update_etag_cache(key, &etag)?;
        Ok(())
    }

    /// 上传字节数据；上传成功后更新本地 .dat/.etag 缓存和内存 ETag 缓存
    pub async fn upload_bytes(&self, key: &str, data: &[u8]) -> Result<(), String> {
        let client = self.get_client()?;
        client.upload_bytes(key, data).await?;
        let metadata = client.get_metadata(key).await?;
        let etag = metadata.etag().to_string();
        self.write_local_cache(key, data, &etag)?;
        self.update_etag_cache(key, &etag)?;
        Ok(())
    }

    /// 下载字节数据（带本地缓存）：
    /// - 若内存 ETag 缓存为空，先通过 list 填充；
    /// - 若本地 .etag 与云端一致，直接读取 .dat 文件返回；
    /// - 否则从 OSS 下载并更新本地缓存和内存缓存。
    pub async fn download_bytes(&self, key: &str) -> Result<Vec<u8>, String> {
        self.ensure_etag_cache().await?;
        let cloud_etag = {
            let cache = self
                .0
                .etag_cache
                .lock()
                .map_err(|_| "锁定ETag缓存失败".to_string())?;
            cache.get(key).cloned()
        };
        let data_path = self.cache_data_path(key)?;
        let etag_path = self.cache_etag_path(key)?;
        if let Some(ref etag) = cloud_etag {
            if data_path.exists() && etag_path.exists() {
                let local_etag = std::fs::read_to_string(&etag_path).unwrap_or_default();
                if &local_etag == etag {
                    return std::fs::read(&data_path)
                        .map_err(|e| format!("读取缓存数据失败: {}", e));
                }
            }
        }
        let client = self.get_client()?;
        let data = client.download_bytes(key).await?;
        let metadata = client.get_metadata(key).await?;
        let etag = metadata.etag().to_string();
        self.write_local_cache(key, &data, &etag)?;
        self.update_etag_cache(key, &etag)?;
        Ok(data)
    }

    /// 下载流数据（带本地缓存）：
    /// - 范围下载（range 不为 None）直接访问 OSS，不使用缓存；
    /// - 完整下载检查本地 .dat/.etag 缓存，命中则以流形式返回缓存数据；
    /// - 缓存未命中时从 OSS 下载，保存到本地缓存并更新内存缓存。
    pub async fn download(
        &self,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<(ByteStream, u64), String> {
        // 范围下载不使用缓存，直接透传给 OssClient
        if range.is_some() {
            return self.get_client()?.download(key, range).await;
        }
        // 完整下载：检查缓存
        self.ensure_etag_cache().await?;
        let cloud_etag = {
            let cache = self
                .0
                .etag_cache
                .lock()
                .map_err(|_| "锁定ETag缓存失败".to_string())?;
            cache.get(key).cloned()
        };
        let data_path = self.cache_data_path(key)?;
        let etag_path = self.cache_etag_path(key)?;
        if let Some(ref etag) = cloud_etag {
            if data_path.exists() && etag_path.exists() {
                let local_etag = std::fs::read_to_string(&etag_path).unwrap_or_default();
                if &local_etag == etag {
                    let data = std::fs::read(&data_path)
                        .map_err(|e| format!("读取缓存数据失败: {}", e))?;
                    let len = data.len() as u64;
                    let bytes = Bytes::from(data);
                    let cached_stream: ByteStream = Box::pin(stream::iter(std::iter::once(
                        Ok::<_, Error>(bytes),
                    )));
                    return Ok((cached_stream, len));
                }
            }
        }
        // 缓存未命中：从 OSS 下载并写入本地缓存
        let client = self.get_client()?;
        let (mut oss_stream, content_len) = client.download(key, None).await?;
        let mut data = Vec::with_capacity(content_len as usize);
        while let Some(chunk) = oss_stream
            .try_next()
            .await
            .map_err(|e| format!("读取下载流失败: {}", e))?
        {
            data.extend_from_slice(&chunk);
        }
        let metadata = client.get_metadata(key).await?;
        let etag = metadata.etag().to_string();
        self.write_local_cache(key, &data, &etag)?;
        self.update_etag_cache(key, &etag)?;
        let len = data.len() as u64;
        let bytes = Bytes::from(data);
        let oss_stream: ByteStream =
            Box::pin(stream::iter(std::iter::once(Ok::<_, Error>(bytes))));
        Ok((oss_stream, len))
    }

    /// 列出对象（委托给 OssClient）
    pub async fn list(
        &self,
        prefix: &str,
        next_token: NextToken,
    ) -> Result<(Vec<ObjectMetadata>, NextToken), String> {
        self.get_client()?.list(prefix, next_token).await
    }

    /// 删除单个对象，并清除对应的本地缓存和内存 ETag 缓存
    pub async fn delete(&self, key: &str) -> Result<(), String> {
        self.get_client()?.delete(key).await?;
        self.remove_from_etag_cache(key)?;
        self.delete_local_cache(key);
        Ok(())
    }

    /// 按前缀批量删除对象，并清除对应的本地缓存和内存 ETag 缓存
    pub async fn delete_with_prefix(&self, prefix: &str) -> Result<u32, String> {
        let client = self.get_client()?;
        let mut next_token = None;
        let mut needs_deletion = Vec::new();
        loop {
            let (objects, nt) = client.list(prefix, next_token).await?;
            for obj in objects {
                needs_deletion.push(obj.key().to_string());
            }
            if nt.is_none() {
                break;
            }
            next_token = nt;
        }
        let total = needs_deletion.len() as u32;
        for key in &needs_deletion {
            client.delete(key).await?;
            self.remove_from_etag_cache(key)?;
            self.delete_local_cache(key);
        }
        Ok(total)
    }

    /// 获取对象元数据（委托给 OssClient）
    pub async fn get_metadata(&self, key: &str) -> Result<ObjectMetadata, String> {
        self.get_client()?.get_metadata(key).await
    }

    /// 生成预签名直链 URL（委托给 OssClient）
    pub fn direct_url(&self, key: &str) -> Result<String, String> {
        self.get_client()?.direct_url(key)
    }
}
