use crate::encryption_manager::EncryptionManager;
use crate::oss_client_manager::OssClientManager;
use crate::secure_diary_store::{DiaryManifest, SecureDiaryStore};
use std::collections::HashMap;
use std::fs::{create_dir_all, read_dir, remove_dir_all, write};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const CACHE_DIARY_DIR: &str = "diary_cache";
const ATTACHMENT_EXTENSION: &str = ".enc";

// 内存缓存：解密后的明文日记列表，用于搜索和展示
// 使用 HashMap 以 ID 为 Key，方便快速查找
pub struct DiaryMemoryCache {
    /// 日记列表，Key 为日记 ID，Value 为解密后的 DiaryManifest
    pub diaries: Mutex<HashMap<String, DiaryManifest>>,
    // 标记数据是否已加载
    pub loaded: Mutex<bool>,
}

impl DiaryMemoryCache {
    pub fn new() -> Self {
        Self {
            diaries: Mutex::new(HashMap::new()),
            loaded: Mutex::new(false),
        }
    }
}

#[derive(Default)]
pub struct AppState {}

impl AppState {
    /// 获取应用的日记缓存目录
    pub fn get_diary_cache_dir(&self, app_handle: &AppHandle) -> PathBuf {
        let path = app_handle
            .path()
            .app_data_dir()
            .unwrap()
            .join(CACHE_DIARY_DIR);

        if !path.exists() {
            create_dir_all(&path).expect("Failed to create diary cache directory");
        }
        path
    }

    // 清空缓存目录内容
    fn clear_cache_dir(&self, cache_dir: &PathBuf) -> Result<(), String> {
        if cache_dir.exists() {
            // 使用 remove_dir_all 删除目录及其内容，再重建
            remove_dir_all(cache_dir)
                .map_err(|e| format!("Failed to delete cache directory: {}", e))?;
        }
        // 重建目录
        create_dir_all(cache_dir)
            .map_err(|e| format!("Failed to create cache directory: {}", e))?;
        Ok(())
    }

    /// 将本地文件加载到内存缓存中
    pub async fn load_cache_to_memory(
        &self,
        cache: &DiaryMemoryCache,
        encryption: &EncryptionManager,
        store: &SecureDiaryStore,
        app_handle: &AppHandle,
    ) -> Result<(), String> {
        let cache_dir = self.get_diary_cache_dir(app_handle);
        let mut map = cache.diaries.lock().unwrap();
        map.clear(); // 清空旧数据，准备加载新数据

        if !cache_dir.exists() {
            *cache.loaded.lock().unwrap() = true;
            return Ok(());
        }

        // 遍历缓存目录下的所有 .enc 文件
        let entries =
            read_dir(cache_dir).map_err(|e| format!("Failed to read cache directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            // 确保只处理 .enc 文件
            if path.extension().and_then(|s| s.to_str()) == Some("enc") {
                // 1. 读取本地密文
                let encrypted_data = std::fs::read(&path)
                    .map_err(|e| format!("Failed to read cached file {}: {}", path.display(), e))?;

                // 2. 解析文件名以获取 UUID (例如从 uuid_etag.enc 中获取 uuid)
                let filename = path.file_stem().unwrap().to_str().unwrap();
                let uuid = filename
                    .rsplit_once('_')
                    .map(|(uuid, _)| uuid)
                    .unwrap_or(filename);

                // 3. 解密和反序列化
                if let Ok(manifest) = store
                    .decrypt_bytes_to_manifest(&encryption, &encrypted_data)
                    .await
                {
                    // 4. 存入内存
                    map.insert(uuid.to_string(), manifest);
                } else {
                    // 记录错误，但继续处理其他文件
                    eprintln!("Warning: Failed to decrypt cached file: {}", path.display());
                }
            }
        }

        *cache.loaded.lock().unwrap() = true;
        Ok(())
    }

    /// 从 OSS 执行全量同步：清空本地缓存，下载所有 Manifest
    pub async fn sync_from_oss(
        &self,
        cache: &DiaryMemoryCache,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        store: &SecureDiaryStore,
        app_handle: &AppHandle,
    ) -> Result<(), String> {
        let cache_dir = self.get_diary_cache_dir(app_handle);

        // 先加载本地文件到内存
        self.load_cache_to_memory(cache, encryption, store, app_handle)
            .await?;

        // 清理本地缓存
        self.clear_cache_dir(&cache_dir)?;

        // 获取远程列表并全部下载到硬盘
        let remote_diaries = store.list_diaries(client).await?;

        for (uuid, diary) in remote_diaries.iter() {
            let remote_etag = diary.etag();

            let new_filename = format!("{}_{}{}", uuid, remote_etag, ATTACHMENT_EXTENSION);
            let new_file_path = cache_dir.join(&new_filename);

            // 并且该方法内部包含了下载和解密逻辑
            let (manifest, manifest_bytes) = store
                .get_diary_manifest(&encryption, &client, uuid.to_string())
                .await?;

            // 写入本地文件系统
            write(&new_file_path, &manifest_bytes)
                .map_err(|e| format!("Failed to write cache file {}: {}", new_filename, e))?;

            // 更新内存缓存
            let mut map = cache.diaries.lock().unwrap();
            map.insert(uuid.to_string(), manifest);
        }

        Ok(())
    }

    pub fn list_cached_diaries(&self, cache: &DiaryMemoryCache) -> Vec<DiaryManifest> {
        let map = cache.diaries.lock().unwrap();
        map.values().cloned().collect()
    }
}
