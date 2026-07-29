use crate::app_config::{AppConfigStore, LocalStorageLocation};
use crate::caches::{LEGACY_LOCAL_OBJECT_STORE_DIRECTORY, LOCAL_OBJECT_STORE_DIRECTORY};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct LocalStorageManager {
    config: AppConfigStore,
    default_root: PathBuf,
    legacy_root: PathBuf,
}

impl LocalStorageManager {
    pub fn new(
        config: AppConfigStore,
        app_local_data_dir: PathBuf,
        app_cache_dir: PathBuf,
    ) -> Self {
        Self {
            config,
            default_root: app_local_data_dir.join(LOCAL_OBJECT_STORE_DIRECTORY),
            legacy_root: app_cache_dir.join(LEGACY_LOCAL_OBJECT_STORE_DIRECTORY),
        }
    }

    pub fn configured_root(&self) -> PathBuf {
        match self.config.current().local_storage_location() {
            LocalStorageLocation::Default => self.default_root.clone(),
            LocalStorageLocation::Custom { base_path } => {
                base_path.join(LOCAL_OBJECT_STORE_DIRECTORY)
            }
        }
    }

    /// 在旧目录尚未迁移时继续使用旧数据，避免升级后出现空日记列表。
    pub fn startup_root(&self) -> PathBuf {
        let configured = self.config.current();
        if matches!(
            configured.local_storage_location(),
            LocalStorageLocation::Default
        ) && !self.default_root.exists()
            && self.legacy_root.exists()
        {
            return self.legacy_root.clone();
        }
        self.configured_root()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppConfig;

    fn manager(
        temp_dir: &tempfile::TempDir,
        location: LocalStorageLocation,
    ) -> LocalStorageManager {
        let config = AppConfigStore::in_memory(AppConfig::with_local_storage_location(location));
        LocalStorageManager::new(
            config,
            temp_dir.path().join("local-data"),
            temp_dir.path().join("cache"),
        )
    }

    #[test]
    fn new_install_uses_persistent_los_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = manager(&temp_dir, LocalStorageLocation::Default);

        assert_eq!(
            manager.startup_root(),
            temp_dir.path().join("local-data").join("los")
        );
    }

    #[test]
    fn legacy_only_install_temporarily_uses_lfc_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = manager(&temp_dir, LocalStorageLocation::Default);
        std::fs::create_dir_all(&manager.legacy_root).unwrap();

        assert_eq!(manager.startup_root(), manager.legacy_root);
    }

    #[test]
    fn completed_default_migration_prefers_los_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = manager(&temp_dir, LocalStorageLocation::Default);
        std::fs::create_dir_all(&manager.legacy_root).unwrap();
        std::fs::create_dir_all(&manager.default_root).unwrap();

        assert_eq!(manager.startup_root(), manager.default_root);
    }

    #[test]
    fn custom_location_appends_only_los_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_path = temp_dir.path().join("chosen");
        let manager = manager(
            &temp_dir,
            LocalStorageLocation::Custom {
                base_path: base_path.clone(),
            },
        );

        assert_eq!(manager.startup_root(), base_path.join("los"));
    }
}
