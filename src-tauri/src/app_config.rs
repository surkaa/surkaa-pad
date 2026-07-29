use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use thiserror::Error;

pub const APP_CONFIG_FILENAME: &str = "app-state.json";
const APP_CONFIG_VERSION: u32 = 1;

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
pub struct AppConfig {
    version: u32,
    #[serde(rename = "localStorageLocation", default)]
    local_storage_location: LocalStorageLocation,
    #[serde(rename = "remoteEnabled", default)]
    remote_enabled: Option<bool>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: APP_CONFIG_VERSION,
            local_storage_location: LocalStorageLocation::Default,
            remote_enabled: None,
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

    #[cfg(test)]
    pub fn with_local_storage_location(location: LocalStorageLocation) -> Self {
        Self {
            local_storage_location: location,
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
    validate_version(config)
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

    if had_current {
        let _ = fs::remove_file(backup_path);
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
