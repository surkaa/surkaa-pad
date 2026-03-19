use std::path::PathBuf;
use crate::caches::{DiaryMemoryCache, LocalFileCache};
use crate::cryptos::Crypto;
use crate::object::OssClient;
use crate::tasks::TaskPool;
use std::sync::{Arc, OnceLock};

#[derive(Clone)]
pub struct AppState {
    crypto: Crypto,
    oss_client_lock: Arc<OnceLock<OssClient>>,
    diary_cache: DiaryMemoryCache,
    local_file_cache: LocalFileCache,
    task_pool: TaskPool,
}

impl AppState {
    pub fn new(path: PathBuf) -> Self {
        let crypto = Crypto::new();
        let diary_cache = DiaryMemoryCache::new();
        let local_file_cache = LocalFileCache::new(path);
        let task_pool = TaskPool::new();
        Self {
            crypto,
            oss_client_lock: Arc::new(OnceLock::new()),
            local_file_cache,
            diary_cache,
            task_pool,
        }
    }

    pub fn crypto(&self) -> Crypto {
        self.crypto.clone()
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
        self.oss_client_lock
            .set(client)
            .map_err(|_| String::from("OssClient 已初始化"))?;
        Ok(())
    }

    pub fn get_client(&self) -> Result<OssClient, String> {
        self.oss_client_lock
            .get()
            .cloned()
            .ok_or(String::from("OssClient 未初始化"))
    }

    pub fn diary_cache(&self) -> DiaryMemoryCache {
        self.diary_cache.clone()
    }
    
    pub fn local_file_cache(&self) -> LocalFileCache {
        self.local_file_cache.clone()
    }

    pub fn task_pool(&self) -> TaskPool {
        self.task_pool.clone()
    }

    pub fn three_state(&self) -> Result<(Crypto, DiaryMemoryCache, LocalFileCache, OssClient), String> {
        Ok((
            self.crypto.clone(),
            self.diary_cache.clone(),
            self.local_file_cache.clone(),
            self.get_client()?,
        ))
    }
}
