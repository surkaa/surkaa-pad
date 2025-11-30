use crate::secure_diary_store::{DiaryManifest, SecureDiaryStore};
use std::collections::HashMap;
use std::fs::{create_dir_all, read_dir, write};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

const CACHE_DIARY_DIR: &str = "diary_cache";

// 内存缓存：解密后的明文日记列表，用于搜索和展示
// 使用 HashMap 以 ID 为 Key，方便快速查找
pub struct DiaryMemoryCache {
    /// 日记列表，Key 为日记 ID，Value 为解密后的 DiaryManifest
    pub diaries: Mutex<HashMap<String, DiaryManifest>>,
    // 标记数据是否已加载
    pub loaded: Mutex<bool>,
}

pub struct AppState {
    pub store: SecureDiaryStore,
    pub cache: DiaryMemoryCache,
    pub app_handle: tauri::AppHandle, // 用于获取路径
}

impl AppState {
    /// 获取应用的日记缓存目录
    fn get_diary_cache_dir(&self) -> PathBuf {
        let path = self
            .app_handle
            .path()
            .app_data_dir()
            .unwrap()
            .join(CACHE_DIARY_DIR);

        if !path.exists() {
            create_dir_all(&path).expect("Failed to create diary cache directory");
        }
        path
    }

    /// 获取本地缓存的所有日记文件名 返回 uuid&etag 文件名格式为 {uuid}_{etag}.enc
    fn get_cached_diary_filenames(&self) -> Result<HashMap<String, String>, String> {
        let cache_dir = self.get_diary_cache_dir();
        if !cache_dir.exists() {
            return Ok(HashMap::new());
        }
        if !cache_dir.is_dir() {
            return Err("Cache directory is not a directory".to_string());
        }
        let mut cached_files = HashMap::new();

        let entries = read_dir(cache_dir)
            .map_err(|e| format!("Failed to read cache directory: {}", e))?;

        for entry in entries {
            let filename = entry
                .map_err(|e| format!("Failed to read directory entry: {}", e))?
                .file_name()
                .to_string_lossy()
                .to_string();
            if let Some((uuid_etag, _)) = filename.rsplit_once('.') {
                if let Some((uuid, etag)) = uuid_etag.rsplit_once('_') {
                    cached_files.insert(uuid.to_string(), etag.to_string());
                }
            }
        }
        Ok(cached_files)
    }

    /// 下载并保存日记文件到本地缓存
    async fn download_and_save(&self, id: &str, file_path: &str) -> Result<(), String> {
        let manifest_bytes = self.store.download_encrypted_manifest(id).await?;
        write(&file_path, &manifest_bytes)
            .map_err(|e| format!("Failed to write cache file: {}", e))?;
        Ok(())
    }

    /// 从 OSS 同步日记到本地缓存
    pub async fn sync_from_oss(&self) -> Result<(), String> {
        let cache_dir = self.get_diary_cache_dir();

        // 获取 OSS 上的所有 ID
        let remote_diaries = self.store.list_diaries().await?;
        // 获取本地缓存的所有
        let cached_files = self.get_cached_diary_filenames()?;

        // 遍历远程 ID
        for diary in remote_diaries.iter() {
            let file_path = cache_dir.join(format!("{}_{}.enc", diary.filename(), diary.etag()));

            if !file_path.exists() {
                // 如果本地不存在该文件，则下载
                self.download_and_save(&diary.filename(), file_path.to_str().unwrap())
                    .await?;
            } else {
                // 如果本地存在该文件，检查 ETag 是否匹配
                if let Some(cached_etag) = cached_files.get(&diary.filename().to_string()) {
                    if cached_etag != &diary.etag() {
                        // ETag 不匹配，重新下载
                        self.download_and_save(&diary.filename(), file_path.to_str().unwrap())
                            .await?;
                    }
                } else {
                    // 本地缓存中没有该文件，重新下载
                    self.download_and_save(&diary.filename(), file_path.to_str().unwrap())
                        .await?;
                }
            }
        }

        // 删除掉本地缓存中不再存在于 OSS 上的日记文件
        for (cached_uuid, _) in cached_files {
            if !remote_diaries.iter().any(|d| d.filename() == cached_uuid) {
                let file_path = cache_dir.join(format!("{}_*.enc", cached_uuid));
                let _ = std::fs::remove_file(file_path);
            }
        }

        Ok(())
    }
}
