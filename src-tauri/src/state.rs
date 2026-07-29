#[cfg(test)]
use crate::app_config::AppConfig;
use crate::app_config::{AppConfigError, AppConfigStore};
use crate::attachments::chunked_upload::ChunkedUploadState;
use crate::attachments::AttachmentServerHandle;
use crate::caches::{DiaryMemoryCache, LocalObjectStore};
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
    local_object_store: LocalObjectStore,
    task_pool: TaskPool,
    chunked_uploads: Arc<DashMap<String, Arc<Mutex<ChunkedUploadState>>>>,
    filename_allocators: Arc<DashMap<String, Arc<Mutex<HashSet<String>>>>>,
    attachment_server: AttachmentServerHandle,
    /// 是否启用远程存储
    remote_enabled: Arc<AtomicBool>,
    storage_mode_gate: Arc<RwLock<()>>,
    app_config: AppConfigStore,
}

impl AppState {
    pub fn new(
        path: PathBuf,
        attachment_server: AttachmentServerHandle,
        app_config: AppConfigStore,
    ) -> Self {
        let crypto = Crypto::new();
        let diary_cache = DiaryMemoryCache::new();
        let local_object_store = LocalObjectStore::new(path);
        let task_pool = TaskPool::new();
        Self {
            crypto,
            oss_client: OssClient::new(),
            local_object_store,
            diary_cache,
            task_pool,
            chunked_uploads: Arc::new(DashMap::new()),
            filename_allocators: Arc::new(DashMap::new()),
            attachment_server,
            remote_enabled: Arc::new(AtomicBool::new(false)),
            storage_mode_gate: Arc::new(RwLock::new(())),
            app_config,
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

    pub fn local_object_store(&self) -> LocalObjectStore {
        self.local_object_store.clone()
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

    pub fn configured_remote_enabled(&self) -> Option<bool> {
        self.app_config.current().remote_enabled()
    }

    pub fn initialize_configured_remote_enabled(
        &self,
        legacy_enabled: bool,
    ) -> Result<bool, AppConfigError> {
        self.app_config.initialize_remote_enabled(legacy_enabled)
    }

    pub fn persist_remote_enabled(&self, enabled: bool) -> Result<(), AppConfigError> {
        self.app_config.set_remote_enabled(enabled)
    }

    /// 根据当前存储模式构造 DiaryStore
    pub fn diary_store(&self) -> Box<dyn DiaryStore> {
        if self.remote_enabled.load(Ordering::Relaxed) {
            Box::new(RemoteStore::new(
                self.local_object_store.clone(),
                self.oss_client.clone(),
            ))
        } else {
            Box::new(LocalStore::new(self.local_object_store.clone()))
        }
    }

    #[cfg(test)]
    pub fn from_parts(
        crypto: Crypto,
        oss_client: OssClient,
        local_object_store: LocalObjectStore,
    ) -> Self {
        Self::from_parts_with_attachment_server(
            crypto,
            oss_client,
            local_object_store,
            AttachmentServerHandle::for_test(),
        )
    }

    #[cfg(test)]
    pub fn from_parts_with_attachment_server(
        crypto: Crypto,
        oss_client: OssClient,
        local_object_store: LocalObjectStore,
        attachment_server: AttachmentServerHandle,
    ) -> Self {
        Self {
            crypto,
            oss_client,
            diary_cache: DiaryMemoryCache::new(),
            local_object_store,
            task_pool: TaskPool::new(),
            chunked_uploads: Arc::new(DashMap::new()),
            filename_allocators: Arc::new(DashMap::new()),
            attachment_server,
            remote_enabled: Arc::new(AtomicBool::new(false)),
            storage_mode_gate: Arc::new(RwLock::new(())),
            app_config: AppConfigStore::in_memory(AppConfig::default()),
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
            LocalObjectStore::new(temp_dir.path().to_path_buf()),
        );

        let operation_guard = state.lock_storage_operation().await;
        assert!(state.try_lock_storage_mode_change().is_none());

        drop(operation_guard);
        assert!(state.try_lock_storage_mode_change().is_some());
    }

    #[test]
    fn configured_remote_mode_is_separate_from_runtime_mode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = AppState::from_parts(
            Crypto::new(),
            OssClient::new(),
            LocalObjectStore::new(temp_dir.path().to_path_buf()),
        );

        assert_eq!(state.configured_remote_enabled(), None);
        assert!(state.initialize_configured_remote_enabled(true).unwrap());
        assert_eq!(state.configured_remote_enabled(), Some(true));
        assert!(!state.is_remote_enabled());

        state.set_remote_enabled(true);
        assert!(state.is_remote_enabled());
    }
}
