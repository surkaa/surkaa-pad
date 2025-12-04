use crate::encryption_manager::EncryptionManager;
use crate::oss_client_manager::{ObjectInfo, OssClientManager};
use crate::surkaa_pad::AppState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::from_slice;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, Manager};
use tauri::path::BaseDirectory;
use tauri_plugin_log::log;
use uuid::Uuid;

const MANIFEST_FILE_NAME: &str = "manifest.enc";
const ATTACHMENT_EXTENSION: &str = ".enc";

#[derive(Default)]
pub struct SecureDiaryStore {}

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
    /// 列出所有日记的主键（也就是创建时间戳）
    pub async fn list_diaries(
        &self,
        client: &OssClientManager,
    ) -> Result<HashMap<String, ObjectInfo>, String> {
        let objects = client
            .list_objects("")
            .await
            .map_err(|e| format!("Failed to list diaries: {}", e))?;
        // 去掉末尾的斜杠和文件名，只保留日记 ID
        let mut unique_objets: HashMap<String, ObjectInfo> = HashMap::new();
        for object in objects {
            if let Some(pos) = object.filename().find('/') {
                // 提取日记 ID（使用切片）
                let diary_id = &object.filename()[..pos];
                // 插入到 HashMap，确保唯一性
                unique_objets.entry(diary_id.to_string()).or_insert(object);
            }
        }
        Ok(unique_objets)
    }

    /// 根据内容创建新的日记并存储到云端
    pub async fn create_diary(
        &self,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        content: &str,
    ) -> Result<DiaryManifest, String> {
        let id = Uuid::new_v4().to_string();
        // 创建一个简单的 manifest
        let manifest = DiaryManifest {
            id: id.clone(),
            algorithm: encryption.algorithm().await,
            content: content.to_string(),
            created: Utc::now().timestamp_millis(),
            updated: Utc::now().timestamp_millis(),
            attachments: Vec::new(),
        };

        // 序列化为 JSON
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;

        // 加密 manifest
        let (ciphertext, nonce) = encryption
            .encrypt(&manifest_json)
            .await
            .map_err(|e| format!("Failed to encrypt manifest: {}", e))?;

        // 组合 nonce 和 ciphertext，前面放 nonce
        let mut encrypted_manifest = nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);

        // 上传到 OSS
        let object_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        client
            .upload_object(&object_key, encrypted_manifest.clone())
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
        client: &OssClientManager,
        id: &str,
    ) -> Result<Vec<u8>, String> {
        let object_key = format!("{}/{}", id, MANIFEST_FILE_NAME);

        client
            .download_object(&object_key)
            .await
            .map_err(|e| format!("Failed to download encrypted manifest for caching: {}", e))
    }

    /// 获取并解密指定 ID 的日记 manifest
    pub async fn get_diary_manifest(
        &self,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        id: String,
    ) -> Result<(DiaryManifest, Vec<u8>), String> {
        let encrypted_data = self.download_encrypted_manifest(client, &id).await?;

        let manifest = self
            .decrypt_bytes_to_manifest(encryption, &encrypted_data)
            .await?;

        Ok((manifest, encrypted_data))
    }

    /// 删除指定 ID 的日记及其所有附件
    pub async fn delete_diary(
        &self,
        client: &OssClientManager,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        id: String,
    ) -> Result<(), String> {
        let objects = client
            .list_objects(&format!("{}/", id))
            .await
            .map_err(|e| format!("Failed to list diary objects: {}", e))?;

        for object in objects {
            client
                .delete_object(&object.filename())
                .await
                .map_err(|e| format!("Failed to delete object {}: {}", object.filename(), e))?;

            // 如果以 MANIFEST_FILE_NAME 结尾，说明是 manifest 文件
            if object.filename().ends_with(MANIFEST_FILE_NAME) {
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
        encryption: &EncryptionManager,
        client: &OssClientManager,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        id: String,
        new_content: &str,
    ) -> Result<DiaryManifest, String> {
        // 先获取现有的 manifest
        let (mut manifest, _) = self
            .get_diary_manifest(encryption, client, id.clone())
            .await?;

        // 更新内容和更新时间
        manifest.content = new_content.to_string();
        manifest.updated = Utc::now().timestamp_millis();

        // 序列化为 JSON
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;

        // 加密 manifest
        let (ciphertext, nonce) = encryption
            .encrypt(&manifest_json)
            .await
            .map_err(|e| format!("Failed to encrypt manifest: {}", e))?;

        // 组合 nonce 和 ciphertext，前面放 nonce
        let mut encrypted_manifest = nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);

        // 上传到 OSS，覆盖原有的 manifest
        let object_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        client
            .upload_object(&object_key, encrypted_manifest.clone())
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
        encryption: &EncryptionManager,
        client: &OssClientManager,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        id: String,
        attachment_bytes: Vec<u8>,
        mime_type: String,
    ) -> Result<DiaryManifest, String> {
        let (encrypted_bytes, nonce) = encryption
            .encrypt(&attachment_bytes)
            .await
            .map_err(|e| format!("Failed to encrypt file key: {}", e))?;

        let file_name = Uuid::new_v4().to_string() + ATTACHMENT_EXTENSION;

        // 创建附件元数据
        let attachment = AttachmentMeta {
            filename: file_name.clone(),
            mimetype: mime_type,
            size: encrypted_bytes.len() as u64,
            nonce: nonce.clone(),
        };

        let (mut manifest, _) = self
            .get_diary_manifest(encryption, client, id.clone())
            .await?;
        manifest.attachments.push(attachment);
        manifest.updated = Utc::now().timestamp_millis();
        let manifest_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
        // 加密
        let (ciphertext, manifest_nonce) = encryption
            .encrypt(&manifest_json)
            .await
            .map_err(|e| format!("Failed to encrypt manifest: {}", e))?;
        let mut encrypted_manifest = manifest_nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);
        // 上传新的 manifest
        client
            .upload_object(&manifest_key, encrypted_manifest.clone())
            .await
            .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

        // 上传附件
        let attachment_key = format!("{}/{}", id, file_name);
        client
            .upload_object(&attachment_key, encrypted_bytes)
            .await
            .map_err(|e| format!("Failed to upload attachment: {}", e))?;

        // 更新本地缓存
        self.replace_local_cache_file(app_state, app_handle, &id, &encrypted_manifest)
            .expect("Failed to update local cache file");

        Ok(manifest)
    }

    /// 下载指定日记的指定附件 下载完成后emit attachment_downloaded返回eid
    pub async fn download_attachment(
        &self,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        app_state: &AppState,
        app_handle: AppHandle,
        id: String,
        filename: String,
        nonce: Vec<u8>,
        eid: String,
    ) -> Result<(), String> {

        // 启动异步下载任务
        let em_clone = encryption.clone();
        let client_clone = client.clone();
        let state_clone = app_state.clone();
        let attachment_key = format!("{}/{}", id, filename);
        tauri::async_runtime::spawn(async move {
            match client_clone.download_object(&attachment_key).await {
                Ok(encrypted_data) => match em_clone.decrypt(&encrypted_data, &nonce).await {
                    Ok(decrypted_data) => {
                        // 保存到临时目录下，再返回给前端临时路径
                        let temp_path = app_handle.path()
                            .resolve(&filename, BaseDirectory::Temp)
                            .unwrap_or_else(|e| {
                                log::error!("无法解析临时目录路径，将使用软件下的attachment_cache目录: {}", e);
                                state_clone.get_attachment_cache_dir(Some(&app_handle)).join(&filename)
                            });
                        tokio::fs::write(&temp_path, &decrypted_data).await
                            .unwrap_or_else(|e| {
                                log::error!("未能将附件写入临时文件 {}: {}", temp_path.display(), e);
                            });

                        log::info!("附件已保存到临时路径: {}", temp_path.display());

                        app_handle.emit(
                            format!("attachment_downloaded_{}", eid).as_str(),
                            serde_json::json!({
                                "eid": eid,
                                "tempPath": temp_path.to_string_lossy(),
                            }),
                        ).unwrap_or_else(|e| {
                            log::error!("未能发出attachment_downloaded事件: {}", e);
                        });
                    }
                    Err(e) => {
                        log::error!("附件解密失败 {}: {}", filename, e);
                    }
                },
                Err(e) => {
                    log::error!("下载附件失败 {}: {}", filename, e);
                }
            }
        });

        Ok(())
    }

    /// 删除指定日记的指定附件
    pub async fn delete_attachment(
        &self,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        app_state: &AppState,
        app_handle: Option<&AppHandle>,
        id: String,
        file_name: String,
    ) -> Result<DiaryManifest, String> {
        // 更新 manifest，移除附件元数据
        let (mut manifest, _) = self
            .get_diary_manifest(encryption, client, id.clone())
            .await?;
        manifest.attachments.retain(|att| att.filename != file_name);
        manifest.updated = Utc::now().timestamp_millis();

        // 序列化
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
        // 加密
        let (ciphertext, manifest_nonce) = encryption
            .encrypt(&manifest_json)
            .await
            .map_err(|e| format!("Failed to encrypt manifest: {}", e))?;
        let mut encrypted_manifest = manifest_nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);
        // 上传更新后的 manifest
        let manifest_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        client
            .upload_object(&manifest_key, encrypted_manifest.clone())
            .await
            .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

        // 删除附件对象
        let attachment_key = format!("{}/{}", id, file_name);
        client
            .delete_object(&attachment_key)
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
        encryption: &EncryptionManager,
        encrypted_data: &Vec<u8>,
    ) -> Result<DiaryManifest, String> {
        let manifest_bytes = encryption
            .decrypt_from_full_ciphertext(encrypted_data)
            .await
            .map_err(|e| format!("Failed to decrypt manifest: {}", e))?;

        // 反序列化 JSON
        let manifest = from_slice(&manifest_bytes)
            .map_err(|e| format!("Failed to parse manifest JSON: {}", e))?;

        Ok(manifest)
    }
}
