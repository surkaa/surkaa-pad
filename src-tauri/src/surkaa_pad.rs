use crate::encryption_manager::EncryptionManager;
use crate::oss_client_manager::OssClientManager;
use crate::secure_diary_store::{DiaryManifest, SecureDiaryStore};
use std::collections::HashMap;
use std::env::current_dir;
use std::fs::{create_dir_all, read_dir, write};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::log;
use tokio::sync::Mutex;

const CACHE_DIARY_DIR: &str = "diary_cache";
const CACHE_ATTACHMENT_DIR: &str = "attachment_cache";
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

#[derive(Default, Clone)]
pub struct AppState {}

impl AppState {
    /// 获取应用的日记缓存目录 TODO 提取成新的函数 参数转成枚举
    pub fn get_diary_cache_dir(&self, app_handle: Option<&AppHandle>) -> PathBuf {
        let path = if let Some(app_handle) = app_handle {
            app_handle
                .path()
                .app_data_dir()
                .unwrap()
                .join(CACHE_DIARY_DIR)
        } else {
            let mut path = current_dir().expect("Failed to get current directory");
            path.push(CACHE_DIARY_DIR);

            path
        };

        if !path.exists() {
            create_dir_all(&path).expect("Failed to create diary cache directory");
        }
        path
    }

    /// 获取应用的附件缓存目录
    pub fn get_attachment_cache_dir(&self, app_handle: Option<&AppHandle>) -> PathBuf {
        let path = if let Some(app_handle) = app_handle {
            app_handle
                .path()
                .app_data_dir()
                .unwrap()
                .join(CACHE_ATTACHMENT_DIR)
        } else {
            let mut path = current_dir().expect("Failed to get current directory");
            path.push(CACHE_ATTACHMENT_DIR);

            path
        };

        if !path.exists() {
            create_dir_all(&path).expect("Failed to create attachment cache directory");
        }
        path
    }

    /// 将本地文件加载到内存缓存中
    pub async fn load_cache_to_memory(
        &self,
        cache: &DiaryMemoryCache,
        encryption: &EncryptionManager,
        store: &SecureDiaryStore,
        app_handle: Option<&AppHandle>,
    ) -> Result<(), String> {
        let cache_dir = self.get_diary_cache_dir(app_handle);
        let mut map = cache.diaries.lock().await;
        map.clear(); // 清空旧数据，准备加载新数据

        if !cache_dir.exists() {
            *cache.loaded.lock().await = true;
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

        *cache.loaded.lock().await = true;
        Ok(())
    }

    /// 从 OSS 执行全量同步：清空本地缓存，下载所有 Manifest
    pub async fn sync_from_oss(
        &self,
        cache: &DiaryMemoryCache,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        store: &SecureDiaryStore,
        app_handle: Option<&AppHandle>,
    ) -> Result<(), String> {
        let cache_dir = self.get_diary_cache_dir(app_handle);

        // 获取远程列表并全部下载到硬盘
        let remote_diaries = store.list_diaries(client).await?;

        for (uuid, diary) in remote_diaries.iter() {
            let remote_etag = diary.etag();

            let new_filename = format!("{}_{}{}", uuid, remote_etag, ATTACHMENT_EXTENSION);
            let new_file_path = cache_dir.join(&new_filename);

            // 判断本地有没有这样的文件，有的话就跳过下载
            if new_file_path.exists() {
                log::info!("Cache hit for diary {}. Skipping download.", uuid);
                continue;
            }

            // 并且该方法内部包含了下载和解密逻辑
            let (manifest, manifest_bytes) = store
                .get_diary_manifest(&encryption, &client, uuid.to_string())
                .await?;

            // 写入本地文件系统
            write(&new_file_path, &manifest_bytes)
                .map_err(|e| format!("Failed to write cache file {}: {}", new_filename, e))?;

            // 更新内存缓存
            let mut map = cache.diaries.lock().await;
            map.insert(uuid.to_string(), manifest);
        }

        // 删除本地多余的（uuid一样，etag却不一样）
        let remote_uuid_for_etag: HashMap<String, String> = remote_diaries
            .iter()
            .map(|(uuid, diary)| (uuid.clone(), diary.etag().to_string()))
            .collect();
        let entries =
            read_dir(&cache_dir).map_err(|e| format!("读取缓存目录失败: {}", e))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("enc") {
                let filename = path.file_stem().unwrap().to_str().unwrap();
                let (uuid, etag) = filename
                    .rsplit_once('_')
                    .ok_or_else(|| format!("无效的缓存文件名格式: {}", filename))?;
                if let Some(remote_etag) = remote_uuid_for_etag.get(uuid) {
                    if remote_etag != etag {
                        // 本地文件的 etag 已经过时，删除它
                        tokio::fs::remove_file(&path)
                            .await
                            .map_err(|e| format!("删除过时缓存文件失败 {}: {}", path.display(), e))?;
                        log::info!("已删除的过时缓存文件: {}", path.display());
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn list_cached_diaries(&self, cache: &DiaryMemoryCache) -> Vec<DiaryManifest> {
        let map = cache.diaries.lock().await;
        map.values().cloned().collect()
    }
}
