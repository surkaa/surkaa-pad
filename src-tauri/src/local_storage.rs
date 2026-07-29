use crate::app_config::{AppConfigStore, LocalStorageLocation, PendingLocalStorageMigration};
use crate::caches::{LEGACY_LOCAL_OBJECT_STORE_DIRECTORY, LOCAL_OBJECT_STORE_DIRECTORY};
use std::path::{Path, PathBuf};

pub mod migration;

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

    pub fn config(&self) -> AppConfigStore {
        self.config.clone()
    }

    pub fn configured_location(&self) -> LocalStorageLocation {
        self.config.current().local_storage_location().clone()
    }

    pub fn pending_migration(&self) -> Option<PendingLocalStorageMigration> {
        self.config
            .current()
            .pending_local_storage_migration()
            .cloned()
    }

    pub fn root_for_location(&self, location: &LocalStorageLocation) -> PathBuf {
        match location {
            LocalStorageLocation::Default => self.default_root.clone(),
            LocalStorageLocation::Custom { base_path } => {
                base_path.join(LOCAL_OBJECT_STORE_DIRECTORY)
            }
        }
    }

    /// 在旧目录尚未迁移时继续使用旧数据，避免升级后出现空日记列表。
    pub fn startup_root(&self) -> PathBuf {
        if let Some(pending) = self.pending_migration() {
            if !pending.source_root().exists() && pending.target_root().exists() {
                return pending.target_root().to_path_buf();
            }
        }
        let configured = self.config.current();
        if matches!(
            configured.local_storage_location(),
            LocalStorageLocation::Default
        ) && (!self.default_root.exists() || directory_is_empty(&self.default_root))
            && self.legacy_root.exists()
        {
            return self.legacy_root.clone();
        }
        self.configured_root()
    }

    pub fn is_legacy_root(&self, root: &std::path::Path) -> bool {
        root == self.legacy_root
    }

    /// 自定义位置一旦完成配置就必须保持可访问，不能像全新默认目录一样按空存储处理。
    pub fn active_root_unavailable_reason(&self, root: &Path) -> Option<String> {
        if !matches!(
            self.configured_location(),
            LocalStorageLocation::Custom { .. }
        ) || root != self.configured_root()
        {
            return None;
        }
        std::fs::read_dir(root).err().map(|error| error.to_string())
    }
}

fn directory_is_empty(path: &PathBuf) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
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
        assert!(manager.is_legacy_root(&manager.startup_root()));
    }

    #[test]
    fn completed_default_migration_prefers_los_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = manager(&temp_dir, LocalStorageLocation::Default);
        std::fs::create_dir_all(&manager.legacy_root).unwrap();
        std::fs::create_dir_all(&manager.default_root).unwrap();
        std::fs::write(manager.default_root.join("migration-complete"), b"ok").unwrap();

        assert_eq!(manager.startup_root(), manager.default_root);
        assert!(!manager.is_legacy_root(&manager.startup_root()));
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

    #[test]
    fn missing_custom_root_is_reported_as_unavailable() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_path = temp_dir.path().join("chosen");
        let manager = manager(
            &temp_dir,
            LocalStorageLocation::Custom {
                base_path: base_path.clone(),
            },
        );
        let root = base_path.join("los");

        assert!(manager.active_root_unavailable_reason(&root).is_some());
        std::fs::create_dir_all(&root).unwrap();
        assert!(manager.active_root_unavailable_reason(&root).is_none());
    }

    #[test]
    fn missing_default_root_is_valid_for_a_new_install() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = manager(&temp_dir, LocalStorageLocation::Default);

        assert!(manager
            .active_root_unavailable_reason(&manager.startup_root())
            .is_none());
    }
}
