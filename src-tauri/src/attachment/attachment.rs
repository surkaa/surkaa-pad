use crate::attachment::types::{AddAttachmentEvent, DownloadAttachmentEvent};
use crate::crypto::Crypto;
use crate::diary::{diary_get, DiaryManifest, DiaryMemoryCache};
use crate::object::{ByteStream, OssClient};
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
    id: &str,
    mimetype: &str,
    encrypt: bool,
    (size, mut stream): (u64, ByteStream),
) {
    // TODO 记得更新缓存
}

pub(super) async fn download_attachment(
    cache: DiaryMemoryCache,
    crypto: Crypto,
    client: OssClient,
    pg: &impl PathGetter,
    event: Arc<dyn MessageSender<DownloadAttachmentEvent>>,
    id: &str,
    filename: String,
) {
}

pub(super) async fn delete_attachment(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    filename: String,
) -> Result<DiaryManifest, String> {
    // 更新 manifest，移除附件元数据
    let mut manifest = diary_get(cache, crypto, client, id).await?;
    manifest.attachments.retain(|att| att.filename != filename);
    manifest.updated = Utc::now().timestamp_millis();

    // 序列化
    let manifest_json = serde_json::to_vec(&manifest)
        .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
    // 加密
    let (ciphertext, manifest_nonce) = crypto.encrypt(&manifest_json)?;
    let mut encrypted_manifest = manifest_nonce;
    encrypted_manifest.extend_from_slice(&ciphertext);
    // 上传更新后的 manifest
    let manifest_key = remote_manifest_key(id);
    client
        .upload_bytes(&manifest_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

    // 删除附件对象
    let attachment_key = remote_attachments_key(id, &filename);
    client
        .delete(&attachment_key)
        .await
        .map_err(|e| format!("Failed to delete attachment: {}", e))?;

    // 获取ETag并更新缓存
    let metadata = client.get_metadata(&manifest_key).await?;
    cache.insert(id, manifest.clone(), metadata.etag().to_string());

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use crate::crypto::Crypto;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_video_attachment() {
        let crypto = Crypto::from_env();
        let test_mp4_full_path = std::env::var("MP4_FILE").expect("未设置视频文件路径");
        let temp_dir = tempdir().expect("无法创建临时目录");
        
    }
}
