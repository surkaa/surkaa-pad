use crate::attachment::types::AddAttachmentEvent;
use crate::attachment::AttachmentMeta;
use crate::crypto::types::EncryptionAlgorithm::Ctr;
use crate::crypto::Crypto;
use crate::diary::{
    delete_diary_attachment, update_diary_attachment, DiaryMemoryCache,
};
use crate::object::tracker_stream::tracker_stream;
use crate::object::{ByteStream, OssClient};
use crate::storage::remote_attachments_key;
use crate::utils::message_sender::MessageSender;
use std::sync::Arc;

pub(super) async fn add_attachment(
    cache: DiaryMemoryCache,
    crypto: Crypto,
    client: OssClient,
    event: Arc<dyn MessageSender<AddAttachmentEvent>>,
    id: &str,
    mimetype: &str,
    encrypted: bool,
    (size, stream): (u64, ByteStream),
) {
    let _ = event.send(AddAttachmentEvent::Started);
    // 直接都使用CTR来加密
    let filename = uuid::Uuid::new_v4().to_string();
    // 包装流 用来更新进度
    let ec = event.clone();
    let stream = tracker_stream(size, stream, move |progress| {
        let _ = ec.send(AddAttachmentEvent::Progress(progress));
    });

    let logic = async move {
        // 直接上传
        let key = remote_attachments_key(id, &filename);
        let attachment = if !encrypted {
            client.upload(&key, size, stream, mimetype).await?;
            // 运行到这里代表上传完成且没有错误
            AttachmentMeta {
                filename,
                mimetype: mimetype.to_string(),
                size,
                nonce: vec![], // 不加密时 nonce 为空
                encrypted: false,
                algorithm: Ctr,
            }
        } else {
            let (stream, nonce) = crypto.encrypt_streaming(stream)?;
            client.upload(&key, size, stream, mimetype).await?;
            // 运行到这里代表上传完成且没有错误
            AttachmentMeta {
                filename,
                mimetype: mimetype.to_string(),
                size,
                nonce,
                encrypted: true,
                algorithm: Ctr,
            }
        };
        // 更新日记
        update_diary_attachment(&cache, &crypto, &client, id, &attachment).await?;
        Ok::<AttachmentMeta, String>(attachment)
    };

    match logic.await {
        Err(e) => {
            let _ = event.send(AddAttachmentEvent::Error(e));
        }
        Ok(attachment) => {
            let _ = event.send(AddAttachmentEvent::Completed(attachment));
        }
    }
}

pub(super) async fn delete_attachment(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    filename: String,
) -> Result<(), String> {
    delete_diary_attachment(cache, crypto, client, id, &filename).await?;

    // 删除附件对象
    let attachment_key = remote_attachments_key(id, &filename);
    client
        .delete(&attachment_key)
        .await
        .map_err(|e| format!("Failed to delete attachment: {}", e))?;

    Ok(())
}
