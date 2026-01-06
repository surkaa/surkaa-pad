use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct CacheFileManager {
    // 缓存文件列表
    cache_files: Arc<Mutex<Vec<PathBuf>>>,
}

impl CacheFileManager {
    pub fn new() -> Self {
        CacheFileManager {
            cache_files: Arc::new(Mutex::new(Vec::new())),
        }
    }

    // 添加缓存文件
    pub fn add_cache_file(&self, path: PathBuf) -> Result<(), String> {
        let mut cache_files = self
            .cache_files
            .lock()
            .map_err(|_| "Failed to acquire lock (poisoned)")?;
        cache_files.push(path);
        Ok(())
    }

    // 获取所有缓存文件
    pub fn get_cache_files(&self) -> Result<Vec<PathBuf>, String> {
        let cache_files = self
            .cache_files
            .lock()
            .map_err(|_| "Failed to acquire lock (poisoned)")?;
        Ok(cache_files.clone())
    }
}
