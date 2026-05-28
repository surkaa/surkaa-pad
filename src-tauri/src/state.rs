use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::Mutex;
use crate::attachments::chunked_upload::ChunkedUploadState;
use crate::caches::{DiaryMemoryCache, LocalFileCache};
use crate::cryptos::Crypto;
use crate::object::OssClient;
use crate::tasks::TaskPool;

#[derive(Clone)]
pub struct AppState {
    crypto: Crypto,
    oss_client: OssClient,
    diary_cache: DiaryMemoryCache,
    local_file_cache: LocalFileCache,
    task_pool: TaskPool,
    chunked_uploads: Arc<DashMap<String, ChunkedUploadState>>,
    /// 每个日记的附件 ID 分配器（MEX 管理并发上传的序号占用）
    attachment_allocators: Arc<DashMap<String, Arc<Mutex<HashSet<u32>>>>>,
    filename_allocators: Arc<DashMap<String, Arc<Mutex<HashSet<String>>>>>,
}

impl AppState {
    pub fn new(path: PathBuf) -> Self {
        let crypto = Crypto::new();
        let diary_cache = DiaryMemoryCache::new();
        let local_file_cache = LocalFileCache::new(path);
        let task_pool = TaskPool::new();
        Self {
            crypto,
            oss_client: OssClient::new(),
            local_file_cache,
            diary_cache,
            task_pool,
            chunked_uploads: Arc::new(DashMap::new()),
            attachment_allocators: Arc::new(DashMap::new()),
            filename_allocators: Arc::new(DashMap::new()),
        }
    }

    pub fn crypto(&self) -> Crypto {
        self.crypto.clone()
    }

    pub fn oss_client(&self) -> OssClient {
        self.oss_client.clone()
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

    pub fn chunked_uploads(&self) -> Arc<DashMap<String, ChunkedUploadState>> {
        self.chunked_uploads.clone()
    }

    pub fn attachment_allocators(&self) -> Arc<DashMap<String, Arc<Mutex<HashSet<u32>>>>> {
        self.attachment_allocators.clone()
    }

    pub fn filename_allocators(&self) -> Arc<DashMap<String, Arc<Mutex<HashSet<String>>>>> {
        self.filename_allocators.clone()
    }

    #[cfg(test)]
    pub fn from_parts(
        crypto: Crypto,
        oss_client: OssClient,
        local_file_cache: LocalFileCache,
    ) -> Self {
        Self {
            crypto,
            oss_client,
            diary_cache: DiaryMemoryCache::new(),
            local_file_cache,
            task_pool: TaskPool::new(),
            chunked_uploads: Arc::new(DashMap::new()),
            attachment_allocators: Arc::new(DashMap::new()),
            filename_allocators: Arc::new(DashMap::new()),
        }
    }
}
