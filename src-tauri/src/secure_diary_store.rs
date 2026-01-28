use crate::crypto::Crypto;
use crate::object::{ObjectMetadata, OssClient};
use crate::surkaa_pad::AppState;
use chrono::Utc;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::from_slice;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::async_runtime::{spawn, JoinHandle};
use tauri::ipc::Channel;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::log;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const MANIFEST_FILE_NAME: &str = "manifest.enc";
const ATTACHMENT_EXTENSION: &str = ".enc";

#[derive(Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum DownloadAttachmentEvent {
    Started { total_size: u64 },
    DownloadProgress { downloaded: u64 },
    Decrypting,
    Decrypted { decrypted_size: u64 },
    Completed { file_path: String },
    Error { message: String },
}

#[derive(Default)]
pub struct SecureDiaryStore {
    // current_download_handle: Arc<Mutex<Option<HashMap<String, JoinHandle<()>>>>>,
    download_handles: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

// Manifest 解密后的 Rust 结构体，代表一篇日记的核心信息
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DiaryManifest {
    pub id: String,
    pub algorithm: String, // 加密算法名称
    pub content: String,   // 日记正文
    pub created: i64,
    pub updated: i64,
    pub attachments: Vec<AttachmentMeta>, // 附件列表
}

// 单个附件的元数据
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mimetype: String,
    pub size: u64,
    pub nonce: Vec<u8>, // 用于加密该文件的独立 IV
}

/// 提供安全的日记存储功能 结合有日记存储方案的逻辑 只用于管理日记及其附件的增删改查
impl SecureDiaryStore {
    pub fn new() -> Self {
        SecureDiaryStore {
            // current_download_handle: Arc::new(Mutex::new(
            //     HashMap::new(),
            // )),
            download_handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 列出所有日记 UUID -> ObjectInfo 映射
    pub async fn list_diaries(
        &self,
        client: Arc<OssClient>,
        uuid: &Option<String>,
    ) -> Result<HashMap<String, ObjectMetadata>, String> {
        let (objects, _) = client
            .list(
                &match uuid {
                    Some(id) => format!("{}/{}", id, MANIFEST_FILE_NAME),
                    None => "".to_string(),
                },
                None,
            )
            .await
            .map_err(|e| format!("Failed to list diaries: {}", e))?;
        // 去掉末尾的斜杠和文件名，只保留日记 ID
        let mut unique_objets: HashMap<String, ObjectMetadata> = HashMap::new();
        for object in objects {
            // 去掉末尾不是以manifest.enc结尾的
            if !object.key().ends_with(MANIFEST_FILE_NAME) {
                continue;
            }
            if let Some(pos) = object.key().find('/') {
                // 提取日记 ID（使用切片）
                let diary_id = &object.key()[..pos];
                // 插入到 HashMap，确保唯一性
                unique_objets.entry(diary_id.to_string()).or_insert(object);
            }
        }
        Ok(unique_objets)
    }

    /// 根据内容创建新的日记并存储到云端
    pub async fn create_diary(
        &self,
        crypto: &Crypto,
        client: Arc<OssClient>,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        content: &str,
    ) -> Result<DiaryManifest, String> {
        let id = Uuid::new_v4().to_string();
        // 创建一个简单的 manifest
        let manifest = DiaryManifest {
            id: id.clone(),
            algorithm: crypto.algorithm().to_string(),
            content: content.to_string(),
            created: Utc::now().timestamp_millis(),
            updated: Utc::now().timestamp_millis(),
            attachments: Vec::new(),
        };

        // 序列化为 JSON
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;

        // 加密 manifest
        let (ciphertext, nonce) = crypto.encrypt(&manifest_json)?;

        // 组合 nonce 和 ciphertext，前面放 nonce
        let mut encrypted_manifest = nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);

        // 上传到 OSS
        let object_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        client
            .upload_bytes(&object_key, &encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload manifest: {}", e))?;

        // 保存到本地
        let digest = md5::compute(&encrypted_manifest);
        let etag = format!("{:X}", digest);
        let filename = format!("{}_{}{}", id, etag, ATTACHMENT_EXTENSION);
        let cache_dir = app_state.get_diary_cache_dir(app_handle);
        let file_path = cache_dir.join(&filename);
        std::fs::write(&file_path, &encrypted_manifest)
            .map_err(|e| format!("Failed to write cache file {}: {}", filename, e))?;

        Ok(manifest)
    }

    /// 直接下载指定 ID 的加密 manifest 字节流用于缓存
    pub async fn download_encrypted_manifest(
        &self,
        client: Arc<OssClient>,
        id: &str,
    ) -> Result<Vec<u8>, String> {
        let object_key = format!("{}/{}", id, MANIFEST_FILE_NAME);

        client
            .download_bytes(&object_key)
            .await
            .map_err(|e| format!("Failed to download encrypted manifest for caching: {}", e))
    }

    /// 获取并解密指定 ID 的日记 manifest
    pub async fn get_diary_manifest(
        &self,
        crypto: &Crypto,
        client: Arc<OssClient>,
        id: String,
    ) -> Result<(DiaryManifest, Vec<u8>), String> {
        let encrypted_data = self.download_encrypted_manifest(client, &id).await?;

        let manifest = self
            .decrypt_bytes_to_manifest(crypto, &encrypted_data)
            .await?;

        Ok((manifest, encrypted_data))
    }

    /// 删除指定 ID 的日记及其所有附件
    pub async fn delete_diary(
        &self,
        client: Arc<OssClient>,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        id: String,
    ) -> Result<(), String> {
        let (objects, _) = client
            .list(&format!("{}/", id), None)
            .await
            .map_err(|e| format!("Failed to list diary objects: {}", e))?;

        for object in objects {
            client
                .delete(&object.key())
                .await
                .map_err(|e| format!("Failed to delete object {}: {}", object.key(), e))?;

            // 如果以 MANIFEST_FILE_NAME 结尾，说明是 manifest 文件
            if object.key().ends_with(MANIFEST_FILE_NAME) {
                let filename = format!("{}_{}{}", id, object.etag(), ATTACHMENT_EXTENSION);
                log::info!("Also deleting cached file {}", filename);
                let cache_dir = app_state.get_diary_cache_dir(app_handle);
                let file_path = cache_dir.join(&filename);
                if file_path.exists() {
                    std::fs::remove_file(&file_path)
                        .map_err(|e| format!("Failed to delete cached file {}: {}", filename, e))?;
                    log::info!("Deleted cached file {}", filename);
                }
            }
        }

        Ok(())
    }

    /// 仅更新日记的文本和元数据，不涉及附件
    pub async fn update_diary_content_only(
        &self,
        crypto: &Crypto,
        client: Arc<OssClient>,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        id: String,
        new_content: &str,
    ) -> Result<DiaryManifest, String> {
        // 先获取现有的 manifest
        let (mut manifest, _) = self
            .get_diary_manifest(crypto, client.clone(), id.clone())
            .await?;

        // 更新内容和更新时间
        manifest.content = new_content.to_string();
        manifest.updated = Utc::now().timestamp_millis();

        // 序列化为 JSON
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;

        // 加密 manifest
        let (ciphertext, nonce) = crypto.encrypt(&manifest_json)?;

        // 组合 nonce 和 ciphertext，前面放 nonce
        let mut encrypted_manifest = nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);

        // 上传到 OSS，覆盖原有的 manifest
        let object_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        client
            .upload_bytes(&object_key, &encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

        // 更新本地缓存
        self.replace_local_cache_file(app_state, app_handle, &id, &encrypted_manifest)
            .expect("Failed to update local cache file");

        Ok(manifest)
    }

    fn replace_local_cache_file(
        &self,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        id: &str,
        new_bytes: &Vec<u8>,
    ) -> Result<(), String> {
        let new_digest = md5::compute(new_bytes);
        let new_etag = format!("{:X}", new_digest);
        let new_filename = format!("{}_{}{}", id, new_etag, ATTACHMENT_EXTENSION);

        let cache_dir = app_state.get_diary_cache_dir(app_handle);

        // 在未知ETag的情况下，删除旧文件，先列出目录中的所有文件
        let entries = std::fs::read_dir(&cache_dir)
            .map_err(|e| format!("Failed to read cache directory: {}", e))?;
        for entry in entries {
            let entry =
                entry.map_err(|e| format!("Failed to read cache directory entry: {}", e))?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            // 检查文件名是否以 id 开头且以 ATTACHMENT_EXTENSION 结尾
            if file_name_str.starts_with(id) && file_name_str.ends_with(ATTACHMENT_EXTENSION) {
                // 删除旧文件
                let old_file_path = cache_dir.join(&file_name);
                std::fs::remove_file(&old_file_path).map_err(|e| {
                    format!("Failed to delete old cache file {}: {}", file_name_str, e)
                })?;
                log::info!("Deleted old cache file {}", file_name_str);
            }
        }

        // 写入新文件
        let new_file_path = cache_dir.join(&new_filename);
        std::fs::write(&new_file_path, new_bytes)
            .map_err(|e| format!("Failed to write new cache file {}: {}", new_filename, e))?;

        Ok(())
    }

    /// 添加附件到指定日记
    pub async fn add_attachment(
        &self,
        crypto: &Crypto,
        client: Arc<OssClient>,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        id: String,
        attachment_bytes: Vec<u8>,
        mime_type: String,
    ) -> Result<DiaryManifest, String> {
        let (encrypted_bytes, nonce) = crypto.encrypt(&attachment_bytes)?;

        let file_name = Uuid::new_v4().to_string() + ATTACHMENT_EXTENSION;

        // 创建附件元数据
        let attachment = AttachmentMeta {
            filename: file_name.clone(),
            mimetype: mime_type,
            size: encrypted_bytes.len() as u64,
            nonce: nonce.clone(),
        };

        let (mut manifest, _) = self
            .get_diary_manifest(crypto, client.clone(), id.clone())
            .await?;
        manifest.attachments.push(attachment);
        manifest.updated = Utc::now().timestamp_millis();
        let manifest_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
        // 加密
        let (ciphertext, manifest_nonce) = crypto.encrypt(&manifest_json)?;
        let mut encrypted_manifest = manifest_nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);
        // 上传新的 manifest
        client
            .upload_bytes(&manifest_key, &encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

        // 上传附件
        let attachment_key = format!("{}/{}", id, file_name);
        client
            .upload_bytes(&attachment_key, &encrypted_bytes)
            .await
            .map_err(|e| format!("Failed to upload attachment: {}", e))?;

        // 更新本地缓存
        self.replace_local_cache_file(app_state, app_handle, &id, &encrypted_manifest)
            .expect("Failed to update local cache file");

        Ok(manifest)
    }

    /// 下载指定日记的指定附件 下载完成后emit attachment_downloaded返回eid
    pub fn download_attachment(
        &self,
        crypto: &Crypto,
        client: Arc<OssClient>,
        app_state: &AppState,
        app_handle: AppHandle,
        event: Channel<DownloadAttachmentEvent>,
        id: String,
        filename: String,
        nonce: Vec<u8>,
        eid: String,
    ) -> Result<PathBuf, String> {
        // 先检查有没有本地缓存，有的话直接返回缓存路径
        let temp_path = app_handle
            .path()
            .resolve(&filename, BaseDirectory::Temp)
            .unwrap_or_else(|e| {
                log::error!(
                    "无法解析临时目录路径，将使用软件下的attachment_cache目录: {}",
                    e
                );
                app_state
                    .get_attachment_cache_dir(Some(&app_handle))
                    .join(&filename)
            });

        if temp_path.exists() {
            // 直接返回缓存路径
            log::info!("附件 {} 已存在于缓存，直接使用缓存文件。", filename);
            let _ = event.send(DownloadAttachmentEvent::Completed {
                file_path: temp_path.to_string_lossy().to_string(),
            });
            return Ok(temp_path);
        }

        // 启动异步下载任务
        let em_clone = crypto.clone();
        let client_clone = client.clone();
        let temp_path_clone = temp_path.clone();
        let attachment_key = format!("{}/{}", id, filename);

        // 克隆用于在任务结束时移除句柄
        let handle_map_clone = self.download_handles.clone();
        let eid_clone = eid.clone();

        let handle = spawn(async move {
            let (mut stream, len) = client_clone
                .download(&attachment_key)
                .await
                .map_err(|e| {
                    let message = format!("Failed to start download: {}", e);
                    log::error!("{}", message.clone());
                    let _ = event.send(DownloadAttachmentEvent::Error { message });
                })
                .unwrap();

            let _ = event.send(DownloadAttachmentEvent::Started { total_size: len });

            let mut downloaded: u64 = 0;
            let mut allocated: Vec<u8> = Vec::with_capacity(len as usize);

            while let Some(chunk) = stream.next().await {
                let chunk = chunk
                    .map_err(|e| {
                        let message = format!("下载时出现错误: {}", e);
                        log::error!("{}", message.clone());
                        let _ = event.send(DownloadAttachmentEvent::Error { message });
                    })
                    .unwrap();
                downloaded += chunk.len() as u64;
                // 发送进度更新事件
                let _ = event.send(DownloadAttachmentEvent::DownloadProgress { downloaded });

                // 存储 chunk 到临时缓冲区
                allocated.extend_from_slice(&chunk);
            }

            // 提示前端下载完成并开始解密
            let _ = event.send(DownloadAttachmentEvent::Decrypting);

            // 解密数据
            let decrypted_data = match em_clone.decrypt(&allocated, &nonce) {
                Ok(data) => data,
                Err(e) => {
                    let message = format!("解密附件时出现错误: {}", e);
                    log::error!("{}", message.clone());
                    let _ = event.send(DownloadAttachmentEvent::Error { message });

                    // 任务结束：无论是成功 (Ok) 还是错误 (Err)，都需要清除句柄
                    let mut handle_guard = handle_map_clone
                        .lock()
                        .map_err(|_| "Failed to acquire lock (poisoned)")
                        .unwrap();
                    handle_guard.remove(&eid);
                    return;
                }
            };

            // 发送解密完成事件
            let decrypted_size = decrypted_data.len() as u64;
            let _ = event.send(DownloadAttachmentEvent::Decrypted { decrypted_size });

            // 保存到临时目录下，再返回给前端临时路径
            let mut temp_file = tokio::fs::File::create(&temp_path_clone)
                .await
                .map_err(|e| {
                    let message = format!("无法创建临时文件 {}: {}", temp_path_clone.display(), e);
                    log::error!("{}", message.clone());
                    let _ = event.send(DownloadAttachmentEvent::Error { message });
                })
                .unwrap();

            // TODO 存的是明文附件，可能有风险，但是目前这点就先不管了，如果存密文的话，打开反而更麻烦
            if let Err(e) = temp_file.write_all(&decrypted_data).await {
                let message = format!("无法写入临时文件 {}: {}", temp_path_clone.display(), e);
                log::error!("{}", message.clone());
                let _ = event.send(DownloadAttachmentEvent::Error { message });
            } else {
                log::info!("附件已保存到临时文件 {}", temp_path_clone.display());
                // 发送完成事件
                let _ = event.send(DownloadAttachmentEvent::Completed {
                    file_path: temp_path_clone.to_string_lossy().to_string(),
                });
            }

            // 任务结束：无论是成功 (Ok) 还是错误 (Err)，都需要清除句柄
            let mut handle_guard = handle_map_clone
                .lock()
                .map_err(|_| "Failed to acquire lock (poisoned)")
                .unwrap();
            handle_guard.remove(&eid);
        });

        // 2. 将新的 JoinHandle 存储到 HashMap 中
        let mut handle_guard = self
            .download_handles
            .lock()
            .map_err(|_| "Failed to acquire lock (poisoned)")?;

        // 如果该 eid 已存在，先取消旧任务 (防止重复下载冲突)
        if let Some(old_handle) = handle_guard.remove(&eid_clone) {
            old_handle.abort();
            log::warn!("发现重复的附件下载任务 {}，已取消旧任务。", &eid_clone);
        }

        handle_guard.insert(eid_clone, handle);

        Ok(temp_path)
    }

    /// 根据 eid 取消对应的下载任务。
    pub fn cancel_download(&self, eid: &str) -> Result<bool, String> {
        // 1. 获取 HashMap 的可变锁
        let mut handle_guard = self
            .download_handles
            .lock()
            .map_err(|_| "Failed to acquire lock (poisoned)")?;

        // 2. 尝试从 HashMap 中取出并移除该 eid 对应的句柄
        if let Some(handle) = handle_guard.remove(eid) {
            // 3. 取消该任务
            handle.abort();
            log::info!("已取消附件下载任务 {}", eid);
            Ok(true)
        } else {
            log::warn!("未找到附件下载任务 {}", eid);
            Ok(false)
        }
    }

    /// 删除指定日记的指定附件
    pub async fn delete_attachment(
        &self,
        crypto: &Crypto,
        client: Arc<OssClient>,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        id: String,
        file_name: String,
    ) -> Result<DiaryManifest, String> {
        // 更新 manifest，移除附件元数据
        let (mut manifest, _) = self
            .get_diary_manifest(crypto, client.clone(), id.clone())
            .await?;
        manifest.attachments.retain(|att| att.filename != file_name);
        manifest.updated = Utc::now().timestamp_millis();

        // 序列化
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
        // 加密
        let (ciphertext, manifest_nonce) = crypto.encrypt(&manifest_json)?;
        let mut encrypted_manifest = manifest_nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);
        // 上传更新后的 manifest
        let manifest_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        client
            .upload_bytes(&manifest_key, &encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

        // 删除附件对象
        let attachment_key = format!("{}/{}", id, file_name);
        client
            .delete(&attachment_key)
            .await
            .map_err(|e| format!("Failed to delete attachment: {}", e))?;

        // 更新本地缓存
        self.replace_local_cache_file(app_state, app_handle, &id, &encrypted_manifest)
            .expect("Failed to update local cache file");

        Ok(manifest)
    }

    /// 将加密的字节流解密为 DiaryManifest 结构体 本应为私有方法 但为了缓存加载需要公开
    pub async fn decrypt_bytes_to_manifest(
        &self,
        crypto: &Crypto,
        encrypted_data: &Vec<u8>,
    ) -> Result<DiaryManifest, String> {
        let manifest_bytes = crypto.decrypt_from_full_ciphertext(encrypted_data)?;

        // 反序列化 JSON
        let manifest = from_slice(&manifest_bytes)
            .map_err(|e| format!("Failed to parse manifest JSON: {}", e))?;

        Ok(manifest)
    }
}
