mod memory_cache;
mod types;

use crate::crypto::Crypto;
use crate::object::{ObjectMetadata, OssClient};
use crate::storage::{is_remote_manifest_key, remote_manifest_key};
pub use memory_cache::MemoryDiaryCache;
use chrono::Utc;
use serde_json::from_slice;
use std::collections::HashMap;
use std::sync::Arc;
use tauri_plugin_log::log;
pub use types::{DiaryManifest};
use uuid::Uuid;

/// 从 OSS 执行全量同步：清空本地缓存，下载所有 Manifest
pub async fn diary_sync(
    dc: &MemoryDiaryCache,
    crypto: &Crypto,
    client: Arc<OssClient>,
    uuid: Option<String>,
) -> Result<Option<DiaryManifest>, String> {
    let (objects, _) = client
        .list(
            &match &uuid {
                Some(id) => remote_manifest_key(id),
                None => "".to_string(),
            },
            None,
        )
        .await?;
    // 去掉末尾的斜杠和文件名，只保留日记 ID
    let mut unique_objets: HashMap<String, ObjectMetadata> = HashMap::new();
    for object in objects {
        // 去掉末尾不是以manifest.enc结尾的
        if !is_remote_manifest_key(&object.key()) {
            continue;
        }
        if let Some(pos) = object.key().find('/') {
            // 提取日记 ID（使用切片）
            let diary_id = &object.key()[..pos];
            // 插入到 HashMap，确保唯一性
            unique_objets.entry(diary_id.to_string()).or_insert(object);
        }
    }
    // 获取远程列表
    let remote_diaries_map: HashMap<String, String> = unique_objets
        .iter()
        .map(|(uuid, diary)| (uuid.clone(), diary.etag().to_string()))
        .collect();
    log::info!(
        "远程日记列表获取成功，共 {} 条日记",
        remote_diaries_map.len()
    );
    // 对比本地和远程的 UUID 和 ETag
    for (uuid, _remote_etag) in remote_diaries_map.iter() {
        // 下载和解密日记Manifest
        let (manifest, _) = diary_get(&crypto, client.clone(), uuid.to_string()).await?;
        // 更新内存缓存
        dc.insert(uuid, manifest);
    }

    // 返回指定 UUID 的 Manifest（如果有的话）
    if let Some(filter_uuid) = uuid {
        Ok(dc.get(&filter_uuid))
    } else {
        Ok(None)
    }
}

/// 根据内容创建新的日记并存储到云端
pub async fn diary_create(
    crypto: &Crypto,
    client: Arc<OssClient>,
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
    client: Arc<OssClient>,
    id: String,
) -> Result<(DiaryManifest, Vec<u8>), String> {
    let object_key = remote_manifest_key(&id);
    let encrypted_data = client
        .download_bytes(&object_key)
        .await
        .map_err(|e| format!("未能下载加密清单用于缓存: {}", e))?;

    let manifest_bytes = crypto.decrypt_from_full_ciphertext(&encrypted_data)?;

    // 反序列化 JSON
    let manifest = from_slice(&manifest_bytes).map_err(|e| format!("未能解析manifest:{}", e))?;

    Ok((manifest, encrypted_data))
}

/// 删除指定 ID 的日记及其所有附件
pub async fn diary_delete(client: Arc<OssClient>, id: String) -> Result<(), String> {
    let (objects, _) = client
        .list(&format!("{}/", id), None)
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

/// 仅更新日记的文本和元数据，不涉及附件
pub async fn diary_update_diary_content_only(
    crypto: &Crypto,
    client: Arc<OssClient>,
    id: String,
    new_content: &str,
) -> Result<DiaryManifest, String> {
    // 先获取现有的 manifest
    let (mut manifest, _) = diary_get(crypto, client.clone(), id.clone()).await?;

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
    let object_key = remote_manifest_key(&id);
    client
        .upload_bytes(&object_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

    Ok(manifest)
}
