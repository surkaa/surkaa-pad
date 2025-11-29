use crate::encryption::EncryptionManager;
use crate::oss_manager::OssClientManager;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::from_slice;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

const MANIFEST_FILE_NAME: &str = "manifest.enc";

pub struct SecureDiaryStore {
    client: Arc<OssClientManager>,
    encryption: EncryptionManager,
}

// Manifest 解密后的 Rust 结构体，代表一篇日记的核心信息
#[derive(Deserialize, Serialize)]
pub struct DiaryManifest {
    pub id: String,
    pub algorithm: String, // 加密算法名称
    pub content: String,   // 日记正文
    pub created_at: i64,
    pub updated_at: i64,
    pub attachments: Vec<AttachmentMeta>, // 附件列表
}

// 单个附件的元数据
#[derive(Deserialize, Serialize)]
pub struct AttachmentMeta {
    pub file_path: String,
    pub mime_type: String,
    pub size: u64,
    pub key: Vec<u8>, // 用于加密该文件的独立 Key
    pub iv: Vec<u8>,  // 用于加密该文件的独立 IV
    pub hash: String, // 原始明文内容的哈希，用于校验
}

impl SecureDiaryStore {
    pub fn new(client: Arc<OssClientManager>, encryption: EncryptionManager) -> Self {
        SecureDiaryStore { client, encryption }
    }

    /// 列出所有日记的主键（也就是创建时间戳）
    pub async fn list_diary_ids(&self) -> Result<Vec<String>, String> {
        let objects = self
            .client
            .list_objects("")
            .await
            .map_err(|e| format!("Failed to list diaries: {}", e))?;
        // 去掉末尾的斜杠和文件名，只保留日记 ID
        let mut unique_ids = HashSet::new();
        for object in objects {
            if let Some(pos) = object.find('/') {
                // 提取日记 ID（使用切片）
                let diary_id = &object[..pos];

                // 将 ID 插入 HashSet。HashSet 自动保证唯一性。
                unique_ids.insert(diary_id.to_string());
            }
        }
        Ok(unique_ids.into_iter().collect())
    }

    /// 根据内容创建新的日记并存储到云端
    pub async fn create_diary(&self, content: &str) -> Result<String, String> {
        let id = Uuid::new_v4().to_string();
        // 创建一个简单的 manifest
        let manifest = DiaryManifest {
            id: id.clone(),
            algorithm: self.encryption.algorithm.clone(),
            content: content.to_string(),
            created_at: Utc::now().timestamp(),
            updated_at: Utc::now().timestamp(),
            attachments: Vec::new(),
        };

        // 序列化为 JSON
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;

        // 加密 manifest
        let (ciphertext, nonce) = self
            .encryption
            .encrypt(&manifest_json)
            .await
            .map_err(|e| format!("Failed to encrypt manifest: {}", e))?;

        // 组合 nonce 和 ciphertext，前面放 nonce
        let mut encrypted_manifest = nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);

        // 上传到 OSS
        let object_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        self.client
            .upload_object(&object_key, encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload manifest: {}", e))?;

        Ok(id)
    }

    /// 获取并解密指定 ID 的日记 manifest
    pub async fn get_diary_manifest(&self, id: String) -> Result<DiaryManifest, String> {
        let object_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        let encrypted_data = self
            .client
            .download_object(&object_key)
            .await
            .map_err(|e| format!("Failed to download manifest: {}", e))?;

        let manifest_bytes = self
            .encryption
            .decrypt_from_full_ciphertext(&encrypted_data)
            .await
            .map_err(|e| format!("Failed to decrypt manifest: {}", e))?;

        // 反序列化 JSON
        let manifest = from_slice(&manifest_bytes)
            .map_err(|e| format!("Failed to parse manifest JSON: {}", e))?;

        Ok(manifest)
    }

    /// 删除指定 ID 的日记及其所有附件
    pub async fn delete_diary(&self, id: String) -> Result<(), String> {
        let objects = self
            .client
            .list_objects(&format!("{}/", id))
            .await
            .map_err(|e| format!("Failed to list diary objects: {}", e))?;

        for object in objects {
            self.client
                .delete_object(&object)
                .await
                .map_err(|e| format!("Failed to delete object {}: {}", object, e))?;
        }

        Ok(())
    }

    /// 仅更新日记的文本和元数据，不涉及附件
    pub async fn update_diary_content_only(&self, id: String, new_content: &str) -> Result<(), String> {
        // 先获取现有的 manifest
        let mut manifest = self.get_diary_manifest(id.clone()).await?;

        // 更新内容和更新时间
        manifest.content = new_content.to_string();
        manifest.updated_at = Utc::now().timestamp();

        // 序列化为 JSON
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;

        // 加密 manifest
        let (ciphertext, nonce) = self
            .encryption
            .encrypt(&manifest_json)
            .await
            .map_err(|e| format!("Failed to encrypt manifest: {}", e))?;

        // 组合 nonce 和 ciphertext，前面放 nonce
        let mut encrypted_manifest = nonce;
        encrypted_manifest.extend_from_slice(&ciphertext);

        // 上传到 OSS，覆盖原有的 manifest
        let object_key = format!("{}/{}", id, MANIFEST_FILE_NAME);
        self.client
            .upload_object(&object_key, encrypted_manifest)
            .await
            .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

        Ok(())
    }
}
