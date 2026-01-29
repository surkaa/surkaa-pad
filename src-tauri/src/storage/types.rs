use std::path::PathBuf;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

/// 获取本地缓存路径的 trait
pub trait PathGetter {
    fn get_cache_path(&self) -> PathBuf;

    fn get_temp_path(&self) -> PathBuf;
}

impl PathGetter for AppHandle {
    fn get_cache_path(&self) -> PathBuf {
        self.path()
            .resolve("", BaseDirectory::AppCache)
            .expect("无法获取 AppCache 路径")
    }

    fn get_temp_path(&self) -> PathBuf {
        self.path()
            .resolve("", BaseDirectory::Temp)
            .expect("无法获取系统 Temp 路径")
    }
}

#[cfg(test)]
pub struct MockCacheGetter {
    root: PathBuf,
}

#[cfg(test)]
impl MockCacheGetter {
    fn new() -> Self {
        use tempfile::tempdir;
        Self {
            root: tempdir().expect("无法创建临时目录").path().to_path_buf(),
        }
    }

    fn with_root(root: PathBuf) -> Self {
        Self { root }
    }
}

#[cfg(test)]
impl PathGetter for MockCacheGetter {
    fn get_cache_path(&self) -> PathBuf {
        self.root.join("cache")
    }

    fn get_temp_path(&self) -> PathBuf {
        self.root.join("temp")
    }
}
