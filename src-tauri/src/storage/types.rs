use std::path::PathBuf;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

/// 获取本地缓存路径的 trait
pub trait PathGetter {
    fn get_data_path(&self) -> PathBuf;
}

impl PathGetter for AppHandle {
    fn get_data_path(&self) -> PathBuf {
        self.path()
            .resolve("", BaseDirectory::AppData)
            .expect("无法获取 AppCache 路径")
    }
}

#[cfg(test)]
pub struct MockGetter {
    root: PathBuf,
}

#[cfg(test)]
impl MockGetter {
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
impl PathGetter for MockGetter {
    fn get_data_path(&self) -> PathBuf {
        self.root.join("cache")
    }
}
