use crate::app_config::{AppConfigStore, LocalStorageLocation, PendingLocalStorageMigration};
use crate::caches::LOCAL_OBJECT_STORE_DIRECTORY;
use std::path::{Path, PathBuf};

pub mod migration;

pub(crate) const MINIMUM_FREE_SPACE_MARGIN: u64 = 1024 * 1024 * 1024;

/// 为一次可能产生新文件的数据写入保留空间。
///
/// 除实际写入量外，至少保留 1 GiB；数据量较大时改用 5% 作为余量。
/// 零写入不需要额外空间。
pub(crate) fn required_space_with_margin(total_bytes: u64) -> u64 {
    if total_bytes == 0 {
        return 0;
    }
    total_bytes.saturating_add(MINIMUM_FREE_SPACE_MARGIN.max(total_bytes / 20))
}

/// 查询指定目录实际所在文件系统的可用空间。
///
/// 目录尚未创建时，从最近的已存在父目录判断，避免错误地固定检查系统盘。
pub(crate) fn available_space_for(path: &Path) -> Result<u64, std::io::Error> {
    let ancestor = existing_ancestor(path).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "找不到目标目录所在磁盘")
    })?;
    fs4::available_space(ancestor)
}

pub(crate) fn existing_ancestor(path: &Path) -> Option<&Path> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.exists() {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

#[derive(Clone, Debug)]
pub struct LocalStorageManager {
    config: AppConfigStore,
    default_root: PathBuf,
}

impl LocalStorageManager {
    pub fn new(config: AppConfigStore, app_local_data_dir: PathBuf) -> Self {
        Self {
            config,
            default_root: app_local_data_dir.join(LOCAL_OBJECT_STORE_DIRECTORY),
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
        self.configured_root()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppConfig;

    fn manager(
        temp_dir: &tempfile::TempDir,
        location: LocalStorageLocation,
    ) -> LocalStorageManager {
        let config = AppConfigStore::in_memory(AppConfig::with_local_storage_location(location));
        LocalStorageManager::new(config, temp_dir.path().join("local-data"))
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
    fn default_location_always_uses_persistent_los_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = manager(&temp_dir, LocalStorageLocation::Default);
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

    #[test]
    fn required_space_adds_a_conservative_margin() {
        assert_eq!(required_space_with_margin(0), 0);
        assert_eq!(
            required_space_with_margin(100),
            100 + MINIMUM_FREE_SPACE_MARGIN
        );
        let large = MINIMUM_FREE_SPACE_MARGIN * 40;
        assert_eq!(required_space_with_margin(large), large + large / 20);
    }

    #[test]
    fn available_space_uses_existing_parent_for_new_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let missing = temp_dir.path().join("not-created").join("los");

        assert!(available_space_for(&missing).unwrap() > 0);
    }
}
