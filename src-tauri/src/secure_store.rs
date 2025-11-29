use crate::encryption::EncryptionManager;
use crate::oss_manager::OssClientManager;
use std::sync::Arc;

use serde_json::from_slice;

const MANIFEST_FILE_NAME: &str = "manifest.enc";

pub struct SecureDiaryStore {
    client: Arc<OssClientManager>,
    encryption: EncryptionManager,
}

// Manifest 解密后的 Rust 结构体，代表一篇日记的核心信息
#[derive(serde::Deserialize)]
pub struct DiaryManifest {
    pub id: i64,
    pub algorithm: String, // 加密算法名称
    pub content: String,   // 日记正文
    pub created_at: i64,
    pub updated_at: i64,
    pub attachments: Vec<AttachmentMeta>, // 附件列表
}

// 单个附件的元数据
#[derive(serde::Deserialize)]
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

    pub async fn list_diary_ids(&self) -> Result<Vec<i64>, String> {
        let objects = self
            .client
            .list_objects("")
            .await
            .map_err(|e| format!("Failed to list diaries: {}", e))?;
        // 将字符串换成 i64 ID 列表
        let mut diary_ids = Vec::new();
        for obj in objects {
            // 先把obj末尾可能的斜杠去掉
            let obj = obj.trim_end_matches('/');
            if let Ok(id) = obj.parse::<i64>() {
                diary_ids.push(id);
            }
        }
        Ok(diary_ids)
    }

    pub async fn get_diary_manifest(&self, id: i64) -> Result<DiaryManifest, String> {
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
}
