use std::ops::Deref;
use crate::attachment::{AttachmentMeta, DownloadAttachmentEvent};
use crate::crypto::Crypto;
use crate::diary::{diary_get, DiaryManifest};
use crate::object::{OssClient, OssState};
use crate::storage::{
    local_attachment_path, remote_attachments_key, remote_manifest_key, PathGetter,
};
use crate::task::TaskPool;
use crate::utils::open_file_stream;
use chrono::Utc;
use futures::StreamExt;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_log::log;
use tokio::fs::{create_dir_all, File};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

/// 给日记添加附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `access_str` - 文件访问路径。
/// * `mimetype` - 附件 MIME 类型
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn add_attachment(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
    access_str: String,
    mimetype: String,
) -> Result<DiaryManifest, String> {
    let client = client.get_client()?;
    // 获取临时文件的完整路径
    let (len, mut stream) = open_file_stream(&access_str)?;

    // 读取流数据到内存
    let mut attachment_bytes: Vec<u8> = Vec::with_capacity(len as usize);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("未能读取附件流:{}", e))?;
        attachment_bytes.extend_from_slice(&chunk);
    }
    // 加密附件数据
    let (encrypted_bytes, nonce) = crypto.encrypt(&attachment_bytes)?;

    let file_name = Uuid::new_v4().to_string();

    // 创建附件元数据
    let attachment = AttachmentMeta {
        filename: file_name.clone(),
        mimetype,
        size: encrypted_bytes.len() as u64,
        nonce: nonce.clone(),
    };

    // 更新 manifest，添加附件元数据
    let (mut manifest, _) = diary_get(crypto.deref(), &client, uuid.clone()).await?;
    manifest.attachments.push(attachment);
    manifest.updated = Utc::now().timestamp_millis();

    // 更新到云端
    let manifest_key = remote_manifest_key(&uuid);
    let manifest_json = serde_json::to_vec(&manifest)
        .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
    let (ciphertext, manifest_nonce) = crypto.encrypt(&manifest_json)?;
    let mut encrypted_manifest = manifest_nonce;
    encrypted_manifest.extend_from_slice(&ciphertext);
    // 上传新的 manifest
    client
        .upload_bytes(&manifest_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

    // 上传附件
    let attachment_key = remote_attachments_key(&uuid, &file_name);
    client
        .upload_bytes(&attachment_key, &encrypted_bytes)
        .await
        .map_err(|e| format!("Failed to upload attachment: {}", e))?;

    Ok(manifest)
}

/// 下载日记附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `filename` - 附件 ID
/// * `nonce` - 解密iv
/// # Returns
/// * `Result<Vec<u8>, String>` - 成功时返回附件字节数据，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn download_attachment(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    app_handle: AppHandle,
    on_event: Channel<DownloadAttachmentEvent>,
    uuid: String,
    filename: String,
    nonce: Vec<u8>,
) -> Result<String, String> {
    let crypto = crypto.inner().clone();
    let client = client.get_client()?;
    tp.spawn(async move {
        download_attachment_inner(
            crypto,
            client,
            &app_handle,
            Arc::new(on_event),
            uuid,
            filename,
            nonce,
        )
        .await;
    })
}

async fn download_attachment_inner(
    crypto: Crypto,
    client: OssClient,
    pg: &impl PathGetter,
    event: Arc<Channel<DownloadAttachmentEvent>>,
    uuid: String,
    filename: String,
    nonce: Vec<u8>, // TODO 考虑删掉这个参数
) {
    let end_event = event.clone();
    let logic = async move {
        // 先检查有没有本地缓存，有的话直接返回缓存路径
        let temp_file_full_path = local_attachment_path(pg, &uuid, &filename);
        if temp_file_full_path.exists() {
            log::info!(
                "附件 {} 已存在于本地缓存，直接返回缓存路径",
                temp_file_full_path.display()
            );
            return Ok(temp_file_full_path.to_string_lossy().to_string());
        }

        let attachment_key = remote_attachments_key(&uuid, &filename);

        let (mut stream, len) = client.download(&attachment_key).await?;

        let _ = event.send(DownloadAttachmentEvent::Started { total_size: len });

        let mut downloaded: u64 = 0;
        let mut allocated: Vec<u8> = Vec::with_capacity(len as usize);

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("下载附件时出错: {}", e))?;
            downloaded += chunk.len() as u64;
            // 发送进度更新事件
            let _ = event.send(DownloadAttachmentEvent::DownloadProgress { downloaded });

            // 存储 chunk 到临时缓冲区
            allocated.extend_from_slice(&chunk);
        }

        // 提示前端下载完成并开始解密
        let _ = event.send(DownloadAttachmentEvent::Decrypting);

        // 解密数据
        let decrypted_data = crypto.decrypt(&allocated, &nonce)?;

        // 发送解密完成事件
        let decrypted_size = decrypted_data.len() as u64;
        let _ = event.send(DownloadAttachmentEvent::Decrypted { decrypted_size });

        // 确保创建了父目录
        if let Some(parent) = temp_file_full_path.parent() {
            create_dir_all(parent)
                .await
                .map_err(|e| format!("无法创建附件临时目录 {}: {}", parent.display(), e))?;
        }

        // 写入临时文件
        let mut temp_file = File::create(&temp_file_full_path)
            .await
            .map_err(|e| format!("无法创建临时文件 {}: {}", temp_file_full_path.display(), e))?;
        temp_file
            .write_all(&decrypted_data)
            .await
            .map_err(|e| format!("无法写入临时文件 {}: {}", temp_file_full_path.display(), e))?;
        log::info!("附件已保存到临时文件 {}", temp_file_full_path.display());

        Ok(temp_file_full_path.to_string_lossy().to_string())
    };

    match logic.await {
        Ok(file_path) => {
            let _ = end_event.send(DownloadAttachmentEvent::Completed { file_path });
        }
        Err(message) => {
            log::error!("附件下载失败: {}", message);
            let _ = end_event.send(DownloadAttachmentEvent::Error { message });
        }
    }
}

/// 删除日记的附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `filename` - 附件 ID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn delete_attachment(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
    file_name: String,
) -> Result<DiaryManifest, String> {
    let client = client.get_client()?;
    // 更新 manifest，移除附件元数据
    let (mut manifest, _) = diary_get(crypto.deref(), &client, uuid.clone()).await?;
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
    let manifest_key = remote_manifest_key(&uuid);
    client
        .upload_bytes(&manifest_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

    // 删除附件对象
    let attachment_key = remote_attachments_key(&uuid, &file_name);
    client
        .delete(&attachment_key)
        .await
        .map_err(|e| format!("Failed to delete attachment: {}", e))?;

    Ok(manifest)
}
