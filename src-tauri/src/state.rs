use crate::attachments::chunked_upload::ChunkedUploadState;
use crate::attachments::AttachmentServerHandle;
use crate::caches::{DiaryMemoryCache, LocalFileCache};
use crate::cryptos::Crypto;
use crate::diaries::{DiaryStore, LocalStore, RemoteStore};
use crate::object::OssClient;
use crate::tasks::TaskPool;
use dashmap::DashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

#[derive(Clone)]
pub struct AppState {
    crypto: Crypto,
    oss_client: OssClient,
    diary_cache: DiaryMemoryCache,
    local_file_cache: LocalFileCache,
    task_pool: TaskPool,
    chunked_uploads: Arc<DashMap<String, Arc<Mutex<ChunkedUploadState>>>>,
    filename_allocators: Arc<DashMap<String, Arc<Mutex<HashSet<String>>>>>,
    attachment_server: AttachmentServerHandle,
    /// 是否启用远程存储
    remote_enabled: Arc<AtomicBool>,
    storage_mode_gate: Arc<RwLock<()>>,
}

impl AppState {
    pub fn new(path: PathBuf, attachment_server: AttachmentServerHandle) -> Self {
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
            filename_allocators: Arc::new(DashMap::new()),
            attachment_server,
            remote_enabled: Arc::new(AtomicBool::new(false)),
            storage_mode_gate: Arc::new(RwLock::new(())),
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

    pub fn chunked_uploads(&self) -> Arc<DashMap<String, Arc<Mutex<ChunkedUploadState>>>> {
        self.chunked_uploads.clone()
    }

    pub async fn lock_storage_operation(&self) -> OwnedRwLockReadGuard<()> {
        self.storage_mode_gate.clone().read_owned().await
    }

    pub fn try_lock_storage_mode_change(&self) -> Option<OwnedRwLockWriteGuard<()>> {
        self.storage_mode_gate.clone().try_write_owned().ok()
    }

    pub fn filename_allocators(&self) -> Arc<DashMap<String, Arc<Mutex<HashSet<String>>>>> {
        self.filename_allocators.clone()
    }

    pub fn attachment_server(&self) -> AttachmentServerHandle {
        self.attachment_server.clone()
    }

    pub fn attachment_url(&self, diary_id: &str, attachment_id: &str) -> String {
        self.attachment_server.url(diary_id, attachment_id)
    }

    /// 是否启用了远程存储
    pub fn is_remote_enabled(&self) -> bool {
        self.remote_enabled.load(Ordering::Relaxed)
    }

    /// 设置远程存储启用状态
    pub fn set_remote_enabled(&self, enabled: bool) {
        self.remote_enabled.store(enabled, Ordering::Relaxed);
    }

    /// 根据当前存储模式构造 DiaryStore
    pub fn diary_store(&self) -> Box<dyn DiaryStore> {
        if self.remote_enabled.load(Ordering::Relaxed) {
            Box::new(RemoteStore::new(
                self.local_file_cache.clone(),
                self.oss_client.clone(),
            ))
        } else {
            Box::new(LocalStore::new(self.local_file_cache.clone()))
        }
    }

    #[cfg(test)]
    pub fn from_parts(
        crypto: Crypto,
        oss_client: OssClient,
        local_file_cache: LocalFileCache,
    ) -> Self {
        Self::from_parts_with_attachment_server(
            crypto,
            oss_client,
            local_file_cache,
            AttachmentServerHandle::for_test(),
        )
    }

    #[cfg(test)]
    pub fn from_parts_with_attachment_server(
        crypto: Crypto,
        oss_client: OssClient,
        local_file_cache: LocalFileCache,
        attachment_server: AttachmentServerHandle,
    ) -> Self {
        Self {
            crypto,
            oss_client,
            diary_cache: DiaryMemoryCache::new(),
            local_file_cache,
            task_pool: TaskPool::new(),
            chunked_uploads: Arc::new(DashMap::new()),
            filename_allocators: Arc::new(DashMap::new()),
            attachment_server,
            remote_enabled: Arc::new(AtomicBool::new(false)),
            storage_mode_gate: Arc::new(RwLock::new(())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn storage_mode_change_waits_for_active_storage_operation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = AppState::from_parts(
            Crypto::new(),
            OssClient::new(),
            LocalFileCache::new(temp_dir.path().to_path_buf()),
        );

        let operation_guard = state.lock_storage_operation().await;
        assert!(state.try_lock_storage_mode_change().is_none());

        drop(operation_guard);
        assert!(state.try_lock_storage_mode_change().is_some());
    }
}
