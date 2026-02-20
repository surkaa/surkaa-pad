use crate::crypto::Crypto;
use crate::object::{OssClient, OssState};
use crate::storage::remote_manifest_key;

use crate::diary::DiaryManifest;
use chrono::Utc;
use serde_json::from_slice;
use std::ops::Deref;
use tauri::State;
use uuid::Uuid;


/// 根据内容保存日记
/// # Arguments
/// * `content` - 日记内容
/// # Returns
/// * `Result<String, String>` - 成功时返回日记 UUID，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn save_diary(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    content: &str,
) -> Result<DiaryManifest, String> {
    let client = client.get_client()?;
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
    let object_key = remote_manifest_key(&id);
    client
        .upload_bytes(&object_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload manifest: {}", e))?;

    Ok(manifest)
}

/// 获取并解密指定 ID 的日记 manifest
pub async fn diary_get(
    crypto: &Crypto,
    client: &OssClient,
    id: String,
) -> Result<DiaryManifest, String> {
    let object_key = remote_manifest_key(&id);
    let encrypted_data = client
        .download_bytes(&object_key)
        .await
        .map_err(|e| format!("未能下载加密清单用于缓存: {}", e))?;

    let manifest_bytes = crypto.decrypt_from_full_ciphertext(&encrypted_data)?;

    // 反序列化 JSON
    let manifest = from_slice(&manifest_bytes).map_err(|e| format!("未能解析manifest:{}", e))?;

    Ok(manifest)
}

/// 删除日记及其所有附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn delete_diary(
    client: State<'_, OssState>,
    uuid: String
) -> Result<(), String> {
    let client = client.get_client()?;
    let (objects, _) = client
        .list(&format!("{}/", uuid), None)
        .await
        .map_err(|e| format!("Failed to list diary objects: {}", e))?;

    for object in objects {
        client
            .delete(&object.key())
            .await
            .map_err(|e| format!("Failed to delete object {}: {}", object.key(), e))?;
    }

    Ok(())
}

/// 更新日记的内容
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `new_content` - 新的日记内容
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn update_diary_content_only(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
    new_content: &str,
) -> Result<DiaryManifest, String> {
    let client = client.get_client()?;
    // 先获取现有的 manifest
    let mut manifest = diary_get(crypto.deref(), &client, uuid.clone()).await?;

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
    let object_key = remote_manifest_key(&uuid);
    client
        .upload_bytes(&object_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

    Ok(manifest)
}
