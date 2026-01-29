pub mod types;

use crate::attachment::types::{AttachmentMeta, DownloadAttachmentEvent, ATTACHMENT_EXTENSION};
use crate::crypto::Crypto;
use crate::diary::diary_get_diary_manifest;
use crate::diary::types::{DiaryManifest, MANIFEST_FILE_NAME};
use crate::object::{OssClient};
use chrono::Utc;
use futures::StreamExt;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::log;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

/// 添加附件到指定日记
pub async fn diary_add_attachment(
    crypto: &Crypto,
    client: Arc<OssClient>,
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

    let (mut manifest, _) = diary_get_diary_manifest(crypto, client.clone(), id.clone()).await?;
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

    Ok(manifest)
}

/// 下载指定日记的指定附件 下载完成后emit attachment_downloaded返回eid
pub async fn diary_download_attachment(
    crypto: Arc<Crypto>,
    client: Arc<OssClient>,
    app_handle: AppHandle,
    event: Channel<DownloadAttachmentEvent>,
    id: String,
    filename: String,
    nonce: Vec<u8>,
) {
    // 先检查有没有本地缓存，有的话直接返回缓存路径
    let temp_path = app_handle
        .path()
        .resolve(&filename, BaseDirectory::Temp)
        .expect("Failed to resolve temp path");

    // 启动异步下载任务
    let em_clone = crypto.clone();
    let client_clone = client.clone();
    let attachment_key = format!("{}/{}", id, filename);

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
            return;
        }
    };

    // 发送解密完成事件
    let decrypted_size = decrypted_data.len() as u64;
    let _ = event.send(DownloadAttachmentEvent::Decrypted { decrypted_size });

    // 保存到临时目录下，再返回给前端临时路径
    let mut temp_file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| {
            let message = format!("无法创建临时文件 {}: {}", temp_path.display(), e);
            log::error!("{}", message.clone());
            let _ = event.send(DownloadAttachmentEvent::Error { message });
        })
        .unwrap();

    // TODO 存的是明文附件，可能有风险，但是目前这点就先不管了，如果存密文的话，打开反而更麻烦
    if let Err(e) = temp_file.write_all(&decrypted_data).await {
        let message = format!("无法写入临时文件 {}: {}", temp_path.display(), e);
        log::error!("{}", message.clone());
        let _ = event.send(DownloadAttachmentEvent::Error { message });
    } else {
        log::info!("附件已保存到临时文件 {}", temp_path.display());
        // 发送完成事件
        let _ = event.send(DownloadAttachmentEvent::Completed {
            file_path: temp_path.to_string_lossy().to_string(),
        });
    }
}

/// 删除指定日记的指定附件
pub async fn diary_delete_attachment(
    crypto: &Crypto,
    client: Arc<OssClient>,
    id: String,
    file_name: String,
) -> Result<DiaryManifest, String> {
    // 更新 manifest，移除附件元数据
    let (mut manifest, _) = diary_get_diary_manifest(crypto, client.clone(), id.clone()).await?;
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

    Ok(manifest)
}
