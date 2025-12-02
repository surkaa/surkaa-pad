use crate::encryption_manager::EncryptionManager;
use crate::oss_client_manager::{ObjectInfo, OssClientManager};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::from_slice;
use std::collections::HashMap;
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
    pub created_at: i64,
    pub updated_at: i64,
    pub attachments: Vec<AttachmentMeta>, // 附件列表
}

// 单个附件的元数据
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct AttachmentMeta {
    pub file_name: String,
    pub mime_type: String,
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
        content: &str,
    ) -> Result<String, String> {
        let id = Uuid::new_v4().to_string();
        // 创建一个简单的 manifest
        let manifest = DiaryManifest {
            id: id.clone(),
            algorithm: encryption.algorithm().await,
            content: content.to_string(),
            created_at: Utc::now().timestamp(),
            updated_at: Utc::now().timestamp(),
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
            .upload_object(&object_key, encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload manifest: {}", e))?;

        Ok(id)
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
    pub async fn delete_diary(&self, client: &OssClientManager, id: String) -> Result<(), String> {
        let objects = client
            .list_objects(&format!("{}/", id))
            .await
            .map_err(|e| format!("Failed to list diary objects: {}", e))?;

        for object in objects {
            client
                .delete_object(&object.filename())
                .await
                .map_err(|e| format!("Failed to delete object {}: {}", object.filename(), e))?;
        }

        Ok(())
    }

    /// 仅更新日记的文本和元数据，不涉及附件
    pub async fn update_diary_content_only(
        &self,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        id: String,
        new_content: &str,
    ) -> Result<(), String> {
        // 先获取现有的 manifest
        let (mut manifest, _) = self
            .get_diary_manifest(encryption, client, id.clone())
            .await?;

        // 更新内容和更新时间
        manifest.content = new_content.to_string();
        manifest.updated_at = Utc::now().timestamp();

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
            .upload_object(&object_key, encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

        Ok(())
    }

    /// 添加附件到指定日记
    pub async fn add_attachment(
        &self,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        id: String,
        attachment_bytes: Vec<u8>,
        mime_type: String,
    ) -> Result<(), String> {
        let (encrypted_bytes, nonce) = encryption
            .encrypt(&attachment_bytes)
            .await
            .map_err(|e| format!("Failed to encrypt file key: {}", e))?;

        let file_name = Uuid::new_v4().to_string() + ATTACHMENT_EXTENSION;

        // 创建附件元数据
        let attachment = AttachmentMeta {
            file_name: file_name.clone(),
            mime_type,
            size: encrypted_bytes.len() as u64,
            nonce: nonce.clone(),
        };

        let (mut manifest, _) = self
            .get_diary_manifest(encryption, client, id.clone())
            .await?;
        manifest.attachments.push(attachment);
        manifest.updated_at = Utc::now().timestamp();
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
            .upload_object(&manifest_key, encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

        // 上传附件
        let attachment_key = format!("{}/{}", id, file_name);
        client
            .upload_object(&attachment_key, encrypted_bytes)
            .await
            .map_err(|e| format!("Failed to upload attachment: {}", e))?;

        Ok(())
    }

    /// 下载指定日记的指定附件
    pub async fn download_attachment(
        &self,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        id: String,
        file_name: String,
        nonce: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let attachment_key = format!("{}/{}", id, file_name);
        let encrypted_data = client
            .download_object(&attachment_key)
            .await
            .map_err(|e| format!("Failed to download attachment: {}", e))?;

        let decrypted_data = encryption
            .decrypt(&encrypted_data, &nonce)
            .await
            .map_err(|e| format!("Failed to decrypt attachment: {}", e))?;

        Ok(decrypted_data)
    }

    /// 删除指定日记的指定附件
    pub async fn delete_attachment(
        &self,
        encryption: &EncryptionManager,
        client: &OssClientManager,
        id: String,
        file_name: String,
    ) -> Result<(), String> {
        // 更新 manifest，移除附件元数据
        let (mut manifest, _) = self
            .get_diary_manifest(encryption, client, id.clone())
            .await?;
        manifest
            .attachments
            .retain(|att| att.file_name != file_name);
        manifest.updated_at = Utc::now().timestamp();

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
            .upload_object(&manifest_key, encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

        // 删除附件对象
        let attachment_key = format!("{}/{}", id, file_name);
        client
            .delete_object(&attachment_key)
            .await
            .map_err(|e| format!("Failed to delete attachment: {}", e))?;

        Ok(())
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
