use crate::error::AppError;
use crate::vault_bootstrap::VaultBootstrap;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use thiserror::Error;

pub const APP_CONFIG_FILENAME: &str = "app-state.json";
pub const DEFAULT_ATTACHMENT_CACHE_LIMIT_BYTES: u64 = 10 * 1024 * 1024 * 1024;
pub const DEFAULT_ATTACHMENT_CACHE_MAX_FILE_SIZE_BYTES: u64 = 100 * 1024 * 1024;
const APP_CONFIG_VERSION: u32 = 1;

fn default_attachment_cache_limit_bytes() -> u64 {
    DEFAULT_ATTACHMENT_CACHE_LIMIT_BYTES
}

fn default_attachment_cache_max_file_size_bytes() -> u64 {
    DEFAULT_ATTACHMENT_CACHE_MAX_FILE_SIZE_BYTES
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LocalStorageLocation {
    #[default]
    Default,
    Custom {
        #[serde(rename = "basePath")]
        base_path: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingLocalStorageMigration {
    #[serde(rename = "sourceRoot")]
    source_root: PathBuf,
    #[serde(rename = "targetRoot")]
    target_root: PathBuf,
    #[serde(rename = "stagingRoot")]
    staging_root: PathBuf,
    #[serde(rename = "targetLocation")]
    target_location: LocalStorageLocation,
}

impl PendingLocalStorageMigration {
    pub fn new(
        source_root: PathBuf,
        target_root: PathBuf,
        staging_root: PathBuf,
        target_location: LocalStorageLocation,
    ) -> Self {
        Self {
            source_root,
            target_root,
            staging_root,
            target_location,
        }
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    pub fn target_root(&self) -> &Path {
        &self.target_root
    }

    pub fn staging_root(&self) -> &Path {
        &self.staging_root
    }

    pub fn target_location(&self) -> &LocalStorageLocation {
        &self.target_location
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    version: u32,
    #[serde(rename = "localStorageLocation", default)]
    local_storage_location: LocalStorageLocation,
    #[serde(rename = "remoteEnabled", default)]
    remote_enabled: Option<bool>,
    #[serde(
        rename = "attachmentCacheLimitBytes",
        default = "default_attachment_cache_limit_bytes"
    )]
    attachment_cache_limit_bytes: u64,
    #[serde(
        rename = "attachmentCacheMaxFileSizeBytes",
        default = "default_attachment_cache_max_file_size_bytes"
    )]
    attachment_cache_max_file_size_bytes: u64,
    #[serde(rename = "pendingLocalStorageMigration", default)]
    pending_local_storage_migration: Option<PendingLocalStorageMigration>,
    #[serde(rename = "vaultBootstrap", default)]
    vault_bootstrap: Option<VaultBootstrap>,
    /// 认证加密的改密记录。与应用配置一起原子保存，不能放在可淘汰的附件缓存中。
    #[serde(rename = "pendingVaultKeyChange", default)]
    pending_vault_key_change: Option<String>,
    /// 本机的匿名性能报告，不包含恢复密钥，也不参与云同步或解锁判断。
    #[serde(
        rename = "lastVaultKeyChangeReport",
        default,
        deserialize_with = "deserialize_key_change_report"
    )]
    last_vault_key_change_report: Option<crate::vault_key_change::KeyChangeReport>,
}

fn deserialize_key_change_report<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<crate::vault_key_change::KeyChangeReport>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    // 可选诊断数据损坏/来自未来版本时忽略，不得因此阻止加载 Vault 和恢复记录。
    Ok(value
        .and_then(|value| {
            serde_json::from_value::<crate::vault_key_change::KeyChangeReport>(value).ok()
        })
        .filter(|report| report.schema_version == 1))
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: APP_CONFIG_VERSION,
            local_storage_location: LocalStorageLocation::Default,
            remote_enabled: None,
            attachment_cache_limit_bytes: DEFAULT_ATTACHMENT_CACHE_LIMIT_BYTES,
            attachment_cache_max_file_size_bytes: DEFAULT_ATTACHMENT_CACHE_MAX_FILE_SIZE_BYTES,
            pending_local_storage_migration: None,
            vault_bootstrap: None,
            pending_vault_key_change: None,
            last_vault_key_change_report: None,
        }
    }
}

impl AppConfig {
    pub fn local_storage_location(&self) -> &LocalStorageLocation {
        &self.local_storage_location
    }

    pub fn remote_enabled(&self) -> Option<bool> {
        self.remote_enabled
    }

    pub fn attachment_cache_limit_bytes(&self) -> u64 {
        self.attachment_cache_limit_bytes
    }

    pub fn attachment_cache_max_file_size_bytes(&self) -> u64 {
        self.attachment_cache_max_file_size_bytes
    }

    pub fn pending_local_storage_migration(&self) -> Option<&PendingLocalStorageMigration> {
        self.pending_local_storage_migration.as_ref()
    }

    pub fn vault_bootstrap(&self) -> Option<&VaultBootstrap> {
        self.vault_bootstrap.as_ref()
    }

    pub fn pending_vault_key_change(&self) -> Option<&str> {
        self.pending_vault_key_change.as_deref()
    }

    pub(crate) fn key_change_report(&self) -> Option<&crate::vault_key_change::KeyChangeReport> {
        self.last_vault_key_change_report.as_ref()
    }

    #[cfg(test)]
    pub fn with_local_storage_location(location: LocalStorageLocation) -> Self {
        Self {
            local_storage_location: location,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub fn with_attachment_cache_limit_bytes(limit_bytes: u64) -> Self {
        Self {
            attachment_cache_limit_bytes: limit_bytes,
            ..Self::default()
        }
    }
}

#[derive(Debug, Error)]
pub enum AppConfigError {
    #[error("读取应用配置失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("解析应用配置失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("不支持的应用配置版本: {0}")]
    UnsupportedVersion(u32),
    #[error("本机另一应用实例正在执行改密，请先关闭它")]
    KeyChangeProcessBusy,
}

impl From<AppConfigError> for AppError {
    fn from(error: AppConfigError) -> Self {
        Self {
            error_type: "app_config".into(),
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppConfigStore {
    path: Option<Arc<PathBuf>>,
    config: Arc<Mutex<AppConfig>>,
}

impl AppConfigStore {
    pub fn load(path: PathBuf) -> Result<Self, AppConfigError> {
        let config = load_config(&path)?;
        Ok(Self {
            path: Some(Arc::new(path)),
            config: Arc::new(Mutex::new(config)),
        })
    }

    #[cfg(test)]
    pub fn in_memory(config: AppConfig) -> Self {
        Self {
            path: None,
            config: Arc::new(Mutex::new(config)),
        }
    }

    pub fn current(&self) -> AppConfig {
        self.lock_config().clone()
    }

    /// 文件锁只保护本机改密执行者；崩溃时 OS 自动释放，云端占位仍保留供续改。
    /// 锁文件固定且不删除，避免删除/重建使并发进程锁住不同文件。
    pub(crate) fn lock_key_change_process(&self) -> Result<Option<fs::File>, AppConfigError> {
        let Some(path) = &self.path else {
            // 只有测试用的纯内存配置没有文件路径。
            return Ok(None);
        };
        let parent = path.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "应用配置路径缺少父目录")
        })?;
        fs::create_dir_all(parent)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("key-change.lock"))?;
        fs4::FileExt::try_lock(&file).map_err(|failure| match failure {
            fs4::TryLockError::WouldBlock => AppConfigError::KeyChangeProcessBusy,
            fs4::TryLockError::Error(error) => AppConfigError::Io(error),
        })?;
        // 上一个执行者可能在另一进程中保存了检查点，不能使用启动时的旧缓存。
        *self.lock_config() = load_config(path)?;
        Ok(Some(file))
    }

    pub fn initialize_remote_enabled(&self, legacy_enabled: bool) -> Result<bool, AppConfigError> {
        if let Some(enabled) = self.current().remote_enabled() {
            return Ok(enabled);
        }
        self.set_remote_enabled(legacy_enabled)?;
        Ok(legacy_enabled)
    }

    pub fn set_remote_enabled(&self, enabled: bool) -> Result<(), AppConfigError> {
        let mut next = self.current();
        next.remote_enabled = Some(enabled);
        self.save(next)
    }

    pub fn set_attachment_cache_limit_bytes(&self, limit_bytes: u64) -> Result<(), AppConfigError> {
        let mut next = self.current();
        next.attachment_cache_limit_bytes = limit_bytes;
        self.save(next)
    }

    pub fn set_attachment_cache_max_file_size_bytes(
        &self,
        limit_bytes: u64,
    ) -> Result<(), AppConfigError> {
        let mut next = self.current();
        next.attachment_cache_max_file_size_bytes = limit_bytes;
        self.save(next)
    }

    pub fn set_vault_bootstrap(&self, bootstrap: VaultBootstrap) -> Result<(), AppConfigError> {
        let mut next = self.current();
        next.vault_bootstrap = Some(bootstrap);
        self.save(next)
    }

    pub fn set_pending_vault_key_change(
        &self,
        journal: Option<String>,
    ) -> Result<(), AppConfigError> {
        self.set_pending_vault_key_change_with_report(journal, None)
    }

    pub(crate) fn set_pending_vault_key_change_with_report(
        &self,
        journal: Option<String>,
        report: Option<crate::vault_key_change::KeyChangeReport>,
    ) -> Result<(), AppConfigError> {
        let mut next = self.current();
        next.pending_vault_key_change = journal.clone();
        if report.is_some() {
            next.last_vault_key_change_report = report;
        }
        let result = self.save(next);
        if result.is_err() && journal.is_some() {
            // 文件写入/备份清理可能只完成了一部分；即使报告失败，也不能开放普通操作。
            self.lock_config().pending_vault_key_change = journal;
        }
        result
    }

    pub(crate) fn set_key_change_report(
        &self,
        report: crate::vault_key_change::KeyChangeReport,
    ) -> Result<(), AppConfigError> {
        let mut next = self.current();
        next.last_vault_key_change_report = Some(report);
        self.save(next)
    }

    pub fn begin_local_storage_migration(
        &self,
        pending: PendingLocalStorageMigration,
    ) -> Result<(), AppConfigError> {
        let mut next = self.current();
        next.pending_local_storage_migration = Some(pending);
        self.save(next)
    }

    pub fn complete_local_storage_migration(
        &self,
        location: LocalStorageLocation,
    ) -> Result<(), AppConfigError> {
        let mut next = self.current();
        next.local_storage_location = location;
        next.pending_local_storage_migration = None;
        self.save(next)
    }

    fn save(&self, config: AppConfig) -> Result<(), AppConfigError> {
        if let Some(path) = &self.path {
            save_config_atomic(path, &config)?;
        }
        *self.lock_config() = config;
        Ok(())
    }

    fn lock_config(&self) -> MutexGuard<'_, AppConfig> {
        self.config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn load_config(path: &Path) -> Result<AppConfig, AppConfigError> {
    let backup_path = backup_path(path);
    let config = match fs::read(path) {
        Ok(bytes) => deserialize_config(&bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match fs::read(&backup_path) {
                Ok(bytes) => deserialize_config(&bytes),
                Err(backup_error) if backup_error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(AppConfig::default());
                }
                Err(backup_error) => Err(backup_error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }?;
    let mut config = validate_version(config)?;
    // 改密确认时，主文件可能已清除记录，但进程在删除旧备份前退出。
    // 不能因此忽略备份中仍存在的恢复密钥桥；继续保持改密锁，直至清理成功。
    if config.pending_vault_key_change.is_none() {
        if let Ok(bytes) = fs::read(&backup_path) {
            if let Ok(backup) = deserialize_config(&bytes) {
                let same_vault = config
                    .vault_bootstrap
                    .as_ref()
                    .zip(backup.vault_bootstrap.as_ref())
                    .is_some_and(|(a, b)| a.vault_id == b.vault_id);
                if same_vault && backup.pending_vault_key_change.is_some() {
                    config.pending_vault_key_change = backup.pending_vault_key_change;
                }
            }
        }
    }
    Ok(config)
}

fn deserialize_config(bytes: &[u8]) -> Result<AppConfig, AppConfigError> {
    Ok(serde_json::from_slice(bytes)?)
}

fn validate_version(config: AppConfig) -> Result<AppConfig, AppConfigError> {
    if config.version != APP_CONFIG_VERSION {
        return Err(AppConfigError::UnsupportedVersion(config.version));
    }
    Ok(config)
}

fn save_config_atomic(path: &Path, config: &AppConfig) -> Result<(), AppConfigError> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "应用配置路径缺少父目录")
    })?;
    fs::create_dir_all(parent)?;

    let temp_path = path.with_extension("json.tmp");
    let backup_path = backup_path(path);
    let bytes = serde_json::to_vec_pretty(config)?;
    let mut temp_file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp_path)?;
    temp_file.write_all(&bytes)?;
    temp_file.sync_all()?;
    drop(temp_file);

    if backup_path.exists() {
        fs::remove_file(&backup_path)?;
    }
    let had_current = path.exists();
    if had_current {
        fs::rename(path, &backup_path)?;
    }

    if let Err(error) = fs::rename(&temp_path, path) {
        if had_current {
            let _ = fs::rename(&backup_path, path);
        }
        return Err(error.into());
    }

    // Android 上目录项也需要持久化，确保新主文件先落盘再删除唯一的旧备份。
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;

    if had_current {
        fs::remove_file(backup_path)?;
    }
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_change_process_lock_probe() {
        let Ok(path) = std::env::var("SURKAA_TEST_KEY_CHANGE_LOCK_PATH") else {
            return;
        };
        let store = AppConfigStore::load(path.into()).unwrap();
        let result = store.lock_key_change_process();
        if std::env::var("SURKAA_TEST_KEY_CHANGE_LOCK_EXPECT").unwrap() == "busy" {
            assert!(matches!(result, Err(AppConfigError::KeyChangeProcessBusy)));
        } else {
            let _guard = result.unwrap().unwrap();
            assert_eq!(
                store.current().pending_vault_key_change(),
                Some("protected-checkpoint")
            );
            if std::env::var("SURKAA_TEST_KEY_CHANGE_LOCK_EXPECT").unwrap() == "exit-with-lock" {
                // 模拟进程突然结束：不运行 File 的 Rust 析构，仍须由 OS 释放锁。
                std::process::exit(0);
            }
        }
    }

    #[test]
    fn key_change_process_lock_excludes_another_process_and_preserves_recovery_after_release() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(APP_CONFIG_FILENAME);
        let store = AppConfigStore::load(path.clone()).unwrap();
        let guard = store.lock_key_change_process().unwrap().unwrap();
        store
            .set_pending_vault_key_change(Some("protected-checkpoint".into()))
            .unwrap();
        let probe = |expected| {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "app_config::tests::key_change_process_lock_probe",
                    "--nocapture",
                ])
                .env("SURKAA_TEST_KEY_CHANGE_LOCK_PATH", &path)
                .env("SURKAA_TEST_KEY_CHANGE_LOCK_EXPECT", expected)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        };
        probe("busy");
        drop(guard);
        probe("exit-with-lock");
        probe("available");
        assert_eq!(
            store.current().pending_vault_key_change(),
            Some("protected-checkpoint")
        );
        assert!(path.with_extension("key-change.lock").exists());
    }

    #[test]
    fn key_change_process_lock_refreshes_stale_state_and_fails_closed_on_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(APP_CONFIG_FILENAME);
        let first = AppConfigStore::load(path.clone()).unwrap();
        let stale = AppConfigStore::load(path.clone()).unwrap();
        let guard = first.lock_key_change_process().unwrap().unwrap();
        first
            .set_pending_vault_key_change(Some("protected-checkpoint".into()))
            .unwrap();
        assert!(matches!(
            stale.lock_key_change_process(),
            Err(AppConfigError::KeyChangeProcessBusy)
        ));
        drop(guard);
        let guard = stale.lock_key_change_process().unwrap().unwrap();
        assert_eq!(
            stale.current().pending_vault_key_change(),
            Some("protected-checkpoint")
        );
        let independent = AppConfigStore::load(temp.path().join("other.json")).unwrap();
        let _independent_guard = independent.lock_key_change_process().unwrap().unwrap();
        drop(guard);
        fs::write(&path, "broken-json").unwrap();
        assert!(stale.lock_key_change_process().is_err());
        assert_eq!(
            stale.current().pending_vault_key_change(),
            Some("protected-checkpoint")
        );
        // 重读失败也释放 OS 锁，修复磁盘配置后可继续，不能永久卡死。
        first
            .set_pending_vault_key_change(Some("protected-checkpoint".into()))
            .unwrap();
        assert!(stale.lock_key_change_process().is_ok());
    }

    #[test]
    fn invalid_optional_diagnostics_do_not_block_security_configuration() {
        for report in [
            serde_json::Value::Null,
            serde_json::json!({"schemaVersion": 99}),
            serde_json::json!("broken-report"),
        ] {
            let mut value = serde_json::to_value(AppConfig::default()).unwrap();
            value["pendingVaultKeyChange"] = "encrypted-journal".into();
            value["lastVaultKeyChangeReport"] = report;
            let config: AppConfig = serde_json::from_value(value).unwrap();
            assert_eq!(config.pending_vault_key_change(), Some("encrypted-journal"));
            assert!(config.key_change_report().is_none());
        }
    }

    #[test]
    fn unfinished_key_change_in_backup_keeps_lock_until_cleanup_is_persisted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(APP_CONFIG_FILENAME);
        let bootstrap = VaultBootstrap {
            schema_version: 1,
            vault_id: "00112233445566778899aabbccddeeff".into(),
            kdf: crate::vault_bootstrap::KeyDerivationParameters::legacy_debug(),
            encrypted_verifier: "not-used-by-this-config-test".into(),
        };
        let main = AppConfig {
            vault_bootstrap: Some(bootstrap),
            ..Default::default()
        };
        let backup = AppConfig {
            pending_vault_key_change: Some("encrypted-journal".into()),
            ..main.clone()
        };
        fs::write(&path, serde_json::to_vec(&main).unwrap()).unwrap();
        fs::write(backup_path(&path), serde_json::to_vec(&backup).unwrap()).unwrap();
        let store = AppConfigStore::load(path.clone()).unwrap();
        assert_eq!(
            store.current().pending_vault_key_change(),
            Some("encrypted-journal")
        );
        store.set_pending_vault_key_change(None).unwrap();
        assert!(!backup_path(&path).exists());
        assert!(AppConfigStore::load(path)
            .unwrap()
            .current()
            .pending_vault_key_change()
            .is_none());
    }

    #[test]
    fn failed_key_change_record_write_still_pauses_current_process() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(APP_CONFIG_FILENAME);
        let store = AppConfigStore::load(path.clone()).unwrap();
        // 无法覆盖目录，模拟初次写入恢复记录失败；没有替换任何用户对象。
        fs::create_dir(path.with_extension("json.tmp")).unwrap();
        assert!(store
            .set_pending_vault_key_change(Some("encrypted-journal".into()))
            .is_err());
        assert_eq!(
            store.current().pending_vault_key_change(),
            Some("encrypted-journal")
        );
    }

    #[test]
    fn missing_config_uses_default_location() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = AppConfigStore::load(temp_dir.path().join(APP_CONFIG_FILENAME)).unwrap();

        assert_eq!(
            store.current().local_storage_location(),
            &LocalStorageLocation::Default
        );
    }

    #[test]
    fn backup_is_used_when_main_config_is_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join(APP_CONFIG_FILENAME);
        let backup = backup_path(&path);
        let config = AppConfig {
            local_storage_location: LocalStorageLocation::Custom {
                base_path: PathBuf::from("E:/BackupData"),
            },
            ..AppConfig::default()
        };
        fs::write(&backup, serde_json::to_vec(&config).unwrap()).unwrap();

        let store = AppConfigStore::load(path).unwrap();

        assert_eq!(store.current(), config);
    }

    #[test]
    fn remote_enabled_is_initialized_once_and_persisted() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join(APP_CONFIG_FILENAME);
        let store = AppConfigStore::load(path.clone()).unwrap();

        assert_eq!(store.current().remote_enabled(), None);
        assert!(store.initialize_remote_enabled(true).unwrap());
        assert!(store.initialize_remote_enabled(false).unwrap());

        let reloaded = AppConfigStore::load(path).unwrap();
        assert_eq!(reloaded.current().remote_enabled(), Some(true));
    }

    #[test]
    fn remote_enabled_can_be_updated_after_initialization() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join(APP_CONFIG_FILENAME);
        let store = AppConfigStore::load(path.clone()).unwrap();

        store.set_remote_enabled(true).unwrap();
        store.set_remote_enabled(false).unwrap();

        let reloaded = AppConfigStore::load(path).unwrap();
        assert_eq!(reloaded.current().remote_enabled(), Some(false));
    }

    #[test]
    fn missing_cache_limit_uses_default_and_updates_persistently() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join(APP_CONFIG_FILENAME);
        fs::write(
            &path,
            br#"{"version":1,"localStorageLocation":{"type":"default"}}"#,
        )
        .unwrap();

        let store = AppConfigStore::load(path.clone()).unwrap();
        assert_eq!(
            store.current().attachment_cache_limit_bytes(),
            DEFAULT_ATTACHMENT_CACHE_LIMIT_BYTES
        );

        store
            .set_attachment_cache_limit_bytes(5 * 1024 * 1024 * 1024)
            .unwrap();
        let reloaded = AppConfigStore::load(path).unwrap();
        assert_eq!(
            reloaded.current().attachment_cache_limit_bytes(),
            5 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn missing_attachment_cache_file_limit_uses_default_and_updates_persistently() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join(APP_CONFIG_FILENAME);
        fs::write(
            &path,
            br#"{"version":1,"localStorageLocation":{"type":"default"}}"#,
        )
        .unwrap();

        let store = AppConfigStore::load(path.clone()).unwrap();
        assert_eq!(
            store.current().attachment_cache_max_file_size_bytes(),
            DEFAULT_ATTACHMENT_CACHE_MAX_FILE_SIZE_BYTES
        );

        store
            .set_attachment_cache_max_file_size_bytes(500 * 1024 * 1024)
            .unwrap();
        let reloaded = AppConfigStore::load(path).unwrap();
        assert_eq!(
            reloaded.current().attachment_cache_max_file_size_bytes(),
            500 * 1024 * 1024
        );
    }

    #[test]
    fn local_storage_migration_is_committed_atomically_with_location() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join(APP_CONFIG_FILENAME);
        let store = AppConfigStore::load(path.clone()).unwrap();
        let location = LocalStorageLocation::Custom {
            base_path: PathBuf::from("D:/DiaryData"),
        };
        let pending = PendingLocalStorageMigration::new(
            PathBuf::from("C:/old/los"),
            PathBuf::from("D:/DiaryData/los"),
            PathBuf::from("D:/DiaryData/los.migrating"),
            location.clone(),
        );

        store.begin_local_storage_migration(pending).unwrap();
        assert!(store.current().pending_local_storage_migration().is_some());

        store
            .complete_local_storage_migration(location.clone())
            .unwrap();

        let reloaded = AppConfigStore::load(path).unwrap();
        assert_eq!(reloaded.current().local_storage_location(), &location);
        assert!(reloaded
            .current()
            .pending_local_storage_migration()
            .is_none());
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join(APP_CONFIG_FILENAME);
        fs::write(
            &path,
            br#"{"version":999,"localStorageLocation":{"type":"default"}}"#,
        )
        .unwrap();

        assert!(matches!(
            AppConfigStore::load(path),
            Err(AppConfigError::UnsupportedVersion(999))
        ));
    }

    #[test]
    fn vault_bootstrap_is_optional_and_persisted() {
        use crate::vault_bootstrap::KeyDerivationParameters;
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join(APP_CONFIG_FILENAME);
        fs::write(
            &path,
            br#"{"version":1,"localStorageLocation":{"type":"default"}}"#,
        )
        .unwrap();
        let store = AppConfigStore::load(path.clone()).unwrap();
        assert!(store.current().vault_bootstrap().is_none());

        let bootstrap = VaultBootstrap {
            schema_version: 1,
            vault_id: "00112233445566778899aabbccddeeff".into(),
            kdf: KeyDerivationParameters::legacy_debug(),
            encrypted_verifier: STANDARD.encode(vec![0; 28]),
        };
        store.set_vault_bootstrap(bootstrap.clone()).unwrap();

        let reloaded = AppConfigStore::load(path).unwrap();
        assert_eq!(reloaded.current().vault_bootstrap(), Some(&bootstrap));
    }
}
