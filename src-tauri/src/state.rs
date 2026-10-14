use crate::ai::AiSessionRepository;
#[cfg(test)]
use crate::app_config::AppConfig;
use crate::app_config::{AppConfigError, AppConfigStore};
use crate::app_object_store::{SharedAppObjectStore, SwitchableAppObjectStore};
use crate::attachments::chunked_upload::ChunkedUploadState;
use crate::attachments::AttachmentServerHandle;
use crate::caches::{AttachmentCacheManager, DiaryMemoryCache, LocalObjectStore};
use crate::cryptos::Crypto;
use crate::diaries::{DiaryStore, LocalStore, RemoteStore};
use crate::local_storage::LocalStorageManager;
use crate::object::OssClient;
use crate::synced_settings::SyncedSettingsRepository;
use crate::tasks::TaskPool;
use crate::vault_bootstrap::{VaultBootstrap, VaultBootstrapRepository};
use dashmap::DashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri_plugin_log::log;
use tokio::sync::{Mutex, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

#[derive(Clone)]
pub struct AppState {
    crypto: Crypto,
    oss_client: OssClient,
    diary_cache: DiaryMemoryCache,
    local_object_store: LocalObjectStore,
    attachment_cache: AttachmentCacheManager,
    task_pool: TaskPool,
    chunked_uploads: Arc<DashMap<String, Arc<Mutex<ChunkedUploadState>>>>,
    filename_allocators: Arc<DashMap<String, Arc<Mutex<HashSet<String>>>>>,
    attachment_server: AttachmentServerHandle,
    /// 是否启用远程存储
    remote_enabled: Arc<AtomicBool>,
    storage_mode_gate: Arc<RwLock<()>>,
    app_config: AppConfigStore,
    local_storage: LocalStorageManager,
    ai_session_repository: AiSessionRepository,
    synced_settings_repository: SyncedSettingsRepository,
    vault_bootstrap_repository: VaultBootstrapRepository,
    key_change_manager: Arc<crate::vault_key_change::KeyChangeManager>,
}

impl AppState {
    pub fn new(
        path: PathBuf,
        attachment_server: AttachmentServerHandle,
        app_config: AppConfigStore,
        local_storage: LocalStorageManager,
    ) -> Self {
        let crypto = Crypto::new();
        let diary_cache = DiaryMemoryCache::new();
        let local_object_store = LocalObjectStore::new(path);
        let attachment_cache =
            AttachmentCacheManager::new(local_object_store.clone(), app_config.clone());
        let task_pool = TaskPool::new();
        let oss_client = OssClient::new();
        let remote_enabled = Arc::new(AtomicBool::new(false));
        let app_object_store: SharedAppObjectStore = Arc::new(SwitchableAppObjectStore::new(
            local_object_store.clone(),
            oss_client.clone(),
            remote_enabled.clone(),
        ));
        let ai_session_repository =
            AiSessionRepository::new(app_object_store.clone(), crypto.clone());
        let synced_settings_repository =
            SyncedSettingsRepository::new(app_object_store, crypto.clone());
        let vault_bootstrap_repository =
            VaultBootstrapRepository::new(app_config.clone(), oss_client.clone(), crypto.clone());
        Self {
            crypto,
            oss_client,
            local_object_store,
            attachment_cache,
            diary_cache,
            task_pool,
            chunked_uploads: Arc::new(DashMap::new()),
            filename_allocators: Arc::new(DashMap::new()),
            attachment_server,
            remote_enabled,
            storage_mode_gate: Arc::new(RwLock::new(())),
            app_config,
            local_storage,
            ai_session_repository,
            synced_settings_repository,
            vault_bootstrap_repository,
            key_change_manager: Arc::new(crate::vault_key_change::KeyChangeManager::default()),
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

    pub fn attachment_cache(&self) -> AttachmentCacheManager {
        self.attachment_cache.clone()
    }

    pub fn task_pool(&self) -> TaskPool {
        self.task_pool.clone()
    }

    pub fn chunked_uploads(&self) -> Arc<DashMap<String, Arc<Mutex<ChunkedUploadState>>>> {
        self.chunked_uploads.clone()
    }

    pub fn ensure_storage_available(&self) -> Result<(), crate::error::AppError> {
        if self.pending_key_change().is_some() {
            return Err(crate::error::AppError {
                error_type: "vault_key_change_pending".into(),
                message: "改密尚未完成，请先继续改密；日记、附件和 AI 操作已暂停".into(),
            });
        }
        Ok(())
    }

    pub async fn lock_storage_operation(
        &self,
    ) -> Result<OwnedRwLockReadGuard<()>, crate::error::AppError> {
        self.ensure_storage_available()?;
        let guard = self.storage_mode_gate.clone().read_owned().await;
        self.ensure_storage_available()?;
        Ok(guard)
    }

    pub fn try_lock_storage_mode_change(&self) -> Option<OwnedRwLockWriteGuard<()>> {
        self.ensure_storage_available().ok()?;
        self.try_lock_key_change()
    }

    pub(crate) fn try_lock_key_change(&self) -> Option<OwnedRwLockWriteGuard<()>> {
        self.storage_mode_gate.clone().try_write_owned().ok()
    }

    pub(crate) fn lock_key_change_process(&self) -> Result<Option<std::fs::File>, AppConfigError> {
        self.app_config.lock_key_change_process()
    }

    pub(crate) fn pending_key_change(&self) -> Option<String> {
        self.app_config
            .current()
            .pending_vault_key_change()
            .map(str::to_owned)
    }

    pub(crate) fn persist_key_change(&self, journal: Option<String>) -> Result<(), AppConfigError> {
        // 合并到已有的检查点写入，不为每个统计事件增加磁盘/云请求。
        match self.key_change_manager.report() {
            Some(report) => self
                .app_config
                .set_pending_vault_key_change_with_report(journal, Some(report)),
            None => self.app_config.set_pending_vault_key_change(journal),
        }
    }

    pub(crate) fn key_change_report(&self) -> Option<crate::vault_key_change::KeyChangeReport> {
        self.key_change_manager
            .report()
            .or_else(|| self.app_config.current().key_change_report().cloned())
    }

    pub(crate) fn persist_key_change_report(&self) {
        if let Some(report) = self.key_change_manager.report() {
            if self.app_config.set_key_change_report(report).is_err() {
                // 统计失败绝不能影响已验证的数据或解除/建立改密锁的安全流程。
                log::warn!("[vault key change] 无法保存匿名性能报告");
            }
        }
    }

    pub(crate) fn key_change_manager(&self) -> Arc<crate::vault_key_change::KeyChangeManager> {
        self.key_change_manager.clone()
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
        let previous = self.remote_enabled.swap(enabled, Ordering::Relaxed);
        if previous != enabled {
            self.ai_session_repository.invalidate_reconciliation();
        }
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

    pub fn vault_bootstrap(&self) -> Option<VaultBootstrap> {
        self.app_config.current().vault_bootstrap().cloned()
    }

    pub fn persist_vault_bootstrap(&self, bootstrap: VaultBootstrap) -> Result<(), AppConfigError> {
        self.app_config.set_vault_bootstrap(bootstrap)
    }

    pub fn attachment_cache_limit_bytes(&self) -> u64 {
        self.app_config.current().attachment_cache_limit_bytes()
    }

    pub fn persist_attachment_cache_limit_bytes(
        &self,
        limit_bytes: u64,
    ) -> Result<(), AppConfigError> {
        self.app_config
            .set_attachment_cache_limit_bytes(limit_bytes)
    }

    pub fn attachment_cache_max_file_size_bytes(&self) -> u64 {
        self.app_config
            .current()
            .attachment_cache_max_file_size_bytes()
    }

    pub fn persist_attachment_cache_max_file_size_bytes(
        &self,
        limit_bytes: u64,
    ) -> Result<(), AppConfigError> {
        self.app_config
            .set_attachment_cache_max_file_size_bytes(limit_bytes)
    }

    pub fn local_storage(&self) -> LocalStorageManager {
        self.local_storage.clone()
    }

    pub fn ai_session_repository(&self) -> AiSessionRepository {
        self.ai_session_repository.clone()
    }

    pub fn synced_settings_repository(&self) -> SyncedSettingsRepository {
        self.synced_settings_repository.clone()
    }

    pub fn vault_bootstrap_repository(&self) -> VaultBootstrapRepository {
        self.vault_bootstrap_repository.clone()
    }

    /// 根据当前存储模式构造 DiaryStore
    pub fn diary_store(&self) -> Box<dyn DiaryStore> {
        if self.remote_enabled.load(Ordering::Relaxed) {
            Box::new(RemoteStore::with_attachment_cache(
                self.local_object_store.clone(),
                self.oss_client.clone(),
                self.attachment_cache.clone(),
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
        let app_config = AppConfigStore::in_memory(AppConfig::default());
        let local_storage =
            LocalStorageManager::new(app_config.clone(), local_object_store.root().to_path_buf());
        let attachment_cache =
            AttachmentCacheManager::new(local_object_store.clone(), app_config.clone());
        let remote_enabled = Arc::new(AtomicBool::new(false));
        let app_object_store: SharedAppObjectStore = Arc::new(SwitchableAppObjectStore::new(
            local_object_store.clone(),
            oss_client.clone(),
            remote_enabled.clone(),
        ));
        let ai_session_repository =
            AiSessionRepository::new(app_object_store.clone(), crypto.clone());
        let synced_settings_repository =
            SyncedSettingsRepository::new(app_object_store, crypto.clone());
        let vault_bootstrap_repository =
            VaultBootstrapRepository::new(app_config.clone(), oss_client.clone(), crypto.clone());
        Self {
            crypto,
            oss_client,
            diary_cache: DiaryMemoryCache::new(),
            local_object_store,
            attachment_cache,
            task_pool: TaskPool::new(),
            chunked_uploads: Arc::new(DashMap::new()),
            filename_allocators: Arc::new(DashMap::new()),
            attachment_server,
            remote_enabled,
            storage_mode_gate: Arc::new(RwLock::new(())),
            app_config,
            local_storage,
            ai_session_repository,
            synced_settings_repository,
            vault_bootstrap_repository,
            key_change_manager: Arc::new(crate::vault_key_change::KeyChangeManager::default()),
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

        let operation_guard = state.lock_storage_operation().await.unwrap();
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

    #[tokio::test]
    async fn ai_repository_follows_runtime_storage_mode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let crypto = Crypto::new();
        crypto
            .derive_dek(
                "state-ai-test-password".into(),
                "c3RhdGUtYWktcmVwb3NpdG9yeS10ZXN0LXNhbHQ",
            )
            .unwrap();
        let state = AppState::from_parts(
            crypto,
            OssClient::new(),
            LocalObjectStore::new(temp_dir.path().to_path_buf()),
        );
        let repository = state.ai_session_repository();
        let session = repository
            .create_session("本地会话".into(), 1)
            .await
            .unwrap();
        assert!(repository.get_session(&session.id).await.unwrap().is_some());

        state.set_remote_enabled(true);
        assert!(repository.get_session(&session.id).await.is_err());

        state.set_remote_enabled(false);
        assert!(repository.get_session(&session.id).await.unwrap().is_some());
    }
}
