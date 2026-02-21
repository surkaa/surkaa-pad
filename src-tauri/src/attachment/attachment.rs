use crate::attachment::types::{AddAttachmentEvent, DownloadAttachmentEvent};
use crate::crypto::Crypto;
use crate::diary::{diary_get, DiaryManifest, DiaryMemoryCache};
use crate::object::OssClient;
use crate::storage::{remote_attachments_key, remote_manifest_key, PathGetter};
use crate::utils::message_sender::MessageSender;
use chrono::Utc;
use std::sync::Arc;

pub(super) async fn add_attachment(
    cache: DiaryMemoryCache,
    crypto: Crypto,
    client: OssClient,
    pg: &impl PathGetter,
    event: Arc<dyn MessageSender<AddAttachmentEvent>>,
    uuid: String,
    access_str: String,
    mimetype: String,
) {
}

pub(super) async fn download_attachment(
    cache: DiaryMemoryCache,
    crypto: Crypto,
    client: OssClient,
    pg: &impl PathGetter,
    event: Arc<dyn MessageSender<DownloadAttachmentEvent>>,
    uuid: String,
    filename: String,
) {
}

pub(super) async fn delete_attachment(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    uuid: String,
    file_name: String,
) -> Result<DiaryManifest, String> {
    // 更新 manifest，移除附件元数据
    let mut manifest = diary_get(cache, crypto, client, uuid.clone()).await?;
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
