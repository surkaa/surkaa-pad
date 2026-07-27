use crate::attachments::attachment_types::AttachmentProcessEvent;
use crate::attachments::{AttachmentError, AttachmentMeta};
use crate::caches::DiaryMemoryCache;
use crate::cryptos::crypto_types::EncryptionAlgorithm::Ctr;
use crate::cryptos::Crypto;
use crate::diaries::diary_store::{
    AttachmentUploadProgress, AttachmentUploadProgressCallback, DiaryStore,
};
use crate::diaries::{
    delete_diary_attachment_locked, get_diary, get_diary_locked, lock_diary_operation,
    update_diary_attachment, update_diary_attachment_filename, update_diary_attachment_locked,
};
use crate::state::AppState;
use crate::stream::ByteStream;
use crate::stream::{collect_data, create_mock_stream, tracker_stream};
use crate::utils::message_sender::MessageSender;
use futures_util::StreamExt;
use image::ImageFormat;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Cursor, Write};
use std::sync::Arc;

fn attachment_upload_progress(
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
) -> AttachmentUploadProgressCallback {
    Arc::new(move |progress| {
        let message = match progress {
            AttachmentUploadProgress::Transferring(percent) => {
                AttachmentProcessEvent::Progress(percent)
            }
            AttachmentUploadProgress::Finalizing => AttachmentProcessEvent::Finalizing,
        };
        let _ = event.send(message);
    })
}

pub fn generate_attachment_id() -> Result<String, AttachmentError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| AttachmentError::IdAssignmentFailed)?;
    Ok(format!(
        "att-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

pub fn deduplicate_filename(desired: &str, existing: &HashSet<String>) -> String {
    let sanitized: String = desired
        .chars()
        .filter(|&c| c != '\0' && c != '/' && c != '\\')
        .collect();
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        return String::from("_file");
    }

    let max_len = 200usize;
    let truncated = if sanitized.len() > max_len {
        let path = std::path::Path::new(sanitized);
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let ext_len = ext.len();
        let stem_max = max_len.saturating_sub(ext_len);
        let stem: String = path
            .file_stem()
            .map(|s| s.to_str().unwrap_or("_file"))
            .unwrap_or("_file")
            .chars()
            .take(stem_max)
            .collect();
        format!("{}{}", stem, ext)
    } else {
        sanitized.to_string()
    };

    if !existing.contains(&truncated) {
        return truncated;
    }

    let path = std::path::Path::new(&truncated);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("_file"));
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    for i in 1..1024 {
        let candidate = format!("{}_{}{}", stem, i, ext);
        if !existing.contains(&candidate) {
            return candidate;
        }
    }

    format!(
        "{}_{}{}",
        stem,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        ext
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn add_attachment_with_result(
    state: &AppState,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    encrypted: bool,
    size: u64,
    mimetype: String,
    stream: ByteStream,
    original_filename: Option<String>,
) -> Result<(AttachmentMeta, String), AttachmentError> {
    // 上传期间固定存储模式，避免启用/关闭云存储与附件事务交错。
    let _storage_mode_guard = state.lock_storage_operation().await;
    let (crypto, cache, store) = (state.crypto(), state.diary_cache(), state.diary_store());
    let upload_progress = attachment_upload_progress(event.clone());

    let logic = async move {
        let str_alloc_state = state
            .filename_allocators()
            .entry(id.to_string())
            .or_default()
            .clone();
        let diary = get_diary(&cache, &crypto, &*store, id)
            .await
            .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;
        let existing_filenames: HashSet<String> = diary
            .attachments
            .iter()
            .map(|attachment| attachment.filename.clone())
            .collect();
        let desired_filename = original_filename.as_deref().unwrap_or("_file");
        let filename = {
            let mut pending_filenames = str_alloc_state.lock().await;
            let combined = existing_filenames
                .union(&pending_filenames)
                .cloned()
                .collect();
            let filename = deduplicate_filename(desired_filename, &combined);
            pending_filenames.insert(filename.clone());
            filename
        };
        let attachment_id = generate_attachment_id()?;

        // 直接上传
        let upload_task = async {
            // 根据是否加密决定最终的流
            let (final_stream, nonce) = if !encrypted {
                (stream, vec![])
            } else {
                crypto.encrypt_streaming(stream)?
            };

            // 通过 store 上传附件（LocalStore 写入 LFC，RemoteStore 写入 OSS + LFC 写透）
            match store
                .upload_attachment_with_progress(
                    id,
                    &attachment_id,
                    size,
                    &mimetype,
                    final_stream,
                    upload_progress,
                )
                .await
            {
                Ok(etag) => Ok(AttachmentMeta {
                    id: attachment_id.clone(),
                    filename: filename.clone(),
                    mimetype,
                    size,
                    nonce,
                    encrypted,
                    algorithm: Ctr,
                    etag: Some(etag),
                }),
                Err(e) => Err(AttachmentError::InvalidOperation(e.to_string())),
            }
        };

        let upload_result: Result<AttachmentMeta, AttachmentError> = upload_task.await;

        let attachment = match upload_result {
            Ok(a) => a,
            Err(e) => {
                let mut pending_filenames = str_alloc_state.lock().await;
                pending_filenames.remove(&filename);
                return Err(e);
            }
        };

        // 更新 Manifest（在 pending 释放前完成，防止并发分配重复 ID）
        if let Err(error) =
            update_diary_attachment(&cache, &crypto, &*store, id, attachment.clone()).await
        {
            let rollback = store.delete_attachment(id, &attachment.id).await;
            str_alloc_state.lock().await.remove(&filename);
            let message = match rollback {
                Ok(()) => error.to_string(),
                Err(rollback_error) => {
                    format!("{error}；附件对象回滚失败：{rollback_error}")
                }
            };
            return Err(AttachmentError::InvalidOperation(message));
        }

        str_alloc_state.lock().await.remove(&filename);

        let url = state.attachment_url(id, &attachment.id);

        Ok::<(AttachmentMeta, String), AttachmentError>((attachment, url))
    };

    logic.await
}

#[allow(clippy::too_many_arguments)]
pub async fn add_attachment(
    state: &AppState,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    encrypted: bool,
    size: u64,
    mimetype: String,
    stream: ByteStream,
    original_filename: Option<String>,
) {
    let _ = event.send(AttachmentProcessEvent::Started);
    match add_attachment_with_result(
        state,
        event.clone(),
        id,
        encrypted,
        size,
        mimetype,
        stream,
        original_filename,
    )
    .await
    {
        Err(e) => {
            let _ = event.send(AttachmentProcessEvent::Error(e.to_string()));
        }
        Ok((attachment, url)) => {
            let _ = event.send(AttachmentProcessEvent::Completed(attachment, url));
        }
    }
}

pub async fn delete_attachment(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    id: &str,
    attachment_id: String,
) -> Result<(), AttachmentError> {
    let guard = lock_diary_operation(id).await;

    delete_diary_attachment_locked(cache, crypto, store, id, &attachment_id, &guard)
        .await
        .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;

    // 删除远端附件对象
    store.delete_attachment(id, &attachment_id).await?;

    Ok(())
}

pub async fn toggle_attachment_encryption(
    state: &AppState,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    attachment_id: String,
) {
    let (crypto, cache, store) = (state.crypto(), state.diary_cache(), state.diary_store());
    let _ = event.send(AttachmentProcessEvent::Started);

    let logic = async {
        let guard = lock_diary_operation(id).await;
        // 获取当前附件信息
        let diary = get_diary_locked(&cache, &crypto, &*store, id, &guard)
            .await
            .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;
        let old_meta = diary
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .ok_or_else(|| AttachmentError::InvalidOperation("附件不存在".to_string()))?
            .clone();

        // 我们需要反转当前的加密状态
        let encrypted = !old_meta.encrypted;

        // 下载原始数据
        let raw_stream = store
            .download_attachment(id, &attachment_id, None, old_meta.etag.as_deref())
            .await?;
        let size = old_meta.size;

        // 处理流转换
        let (processed_stream, new_nonce) = if old_meta.encrypted && !encrypted {
            // 解密：从加密转为明文
            let decrypted = crypto.decrypt_streaming(raw_stream, &old_meta.nonce, 0)?;
            (decrypted, vec![])
        } else if !old_meta.encrypted && encrypted {
            // 加密：从明文转为加密
            let (encrypted_stream, nonce) = crypto.encrypt_streaming(raw_stream)?;
            (encrypted_stream, nonce)
        } else {
            return Err(AttachmentError::InvalidOperation(
                "无效的转换状态".to_string(),
            ));
        };

        // 通过 store 上传
        let new_etag = store
            .upload_attachment_with_progress(
                id,
                &attachment_id,
                size,
                &old_meta.mimetype,
                processed_stream,
                attachment_upload_progress(event.clone()),
            )
            .await
            .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;

        // 构造新的元数据并更新 Manifest
        let mut new_meta = old_meta.clone();
        new_meta.encrypted = encrypted;
        new_meta.nonce = new_nonce;
        new_meta.etag = Some(new_etag);

        update_diary_attachment_locked(&cache, &crypto, &*store, id, new_meta.clone(), &guard)
            .await
            .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;

        let url = state.attachment_url(id, &new_meta.id);
        Ok((new_meta, url))
    };

    match logic.await {
        Ok((meta, url)) => {
            let _ = event.send(AttachmentProcessEvent::Completed(meta, url));
        }
        Err(e) => {
            let _ = event.send(AttachmentProcessEvent::Error(e.to_string()));
        }
    }
}

pub async fn rotate_image_attachment(
    state: &AppState,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    attachment_id: String,
    rotation: i32,
) {
    let (crypto, cache, store) = (state.crypto(), state.diary_cache(), state.diary_store());
    let _ = event.send(AttachmentProcessEvent::Started);

    let logic = async {
        let guard = lock_diary_operation(id).await;
        // 检测 rotation 参数是否合法
        if ![90, -90, 180].contains(&rotation) {
            return Err(AttachmentError::InvalidOperation(
                "不支持的旋转角度，仅支持 90, -90, 180".to_string(),
            ));
        }
        // 获取元数据
        let diary = get_diary_locked(&cache, &crypto, &*store, id, &guard)
            .await
            .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;
        let old_meta = diary
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .ok_or_else(|| AttachmentError::InvalidOperation("附件不存在".to_string()))?
            .clone();

        // 验证 MIME 类型是否为图片
        if !old_meta.mimetype.starts_with("image/") {
            return Err(AttachmentError::InvalidOperation(
                "附件不是图片，无法旋转".to_string(),
            ));
        }

        // 下载并解密原始数据
        let raw_stream = store
            .download_attachment(id, &attachment_id, None, old_meta.etag.as_deref())
            .await?;

        let stream = if old_meta.encrypted {
            crypto.decrypt_streaming(raw_stream, &old_meta.nonce, 0)?
        } else {
            raw_stream
        };

        // 将流收集到内存 图片处理必须在内存中进行
        let buffer = collect_data(stream).await?;

        // 使用 image 库处理旋转
        let img = image::load_from_memory(&buffer)
            .map_err(|e| AttachmentError::ImageProcessingFailed(format!("图片解码失败: {}", e)))?;

        let rotated_img = match rotation {
            90 => img.rotate90(),
            180 => img.rotate180(),
            -90 => img.rotate270(),
            _ => {
                return Err(AttachmentError::InvalidOperation(
                    "不支持的旋转角度，仅支持 90, 180, -90".to_string(),
                ))
            }
        };

        // 4. 将旋转后的图片编码回字节流
        let mut output_buffer = Vec::new();
        let format = match old_meta.mimetype.as_str() {
            "image/jpeg" | "image/jpg" => ImageFormat::Jpeg,
            "image/png" => ImageFormat::Png,
            "image/webp" => ImageFormat::WebP,
            _ => ImageFormat::Png, // 默认 PNG
        };

        rotated_img
            .write_to(&mut Cursor::new(&mut output_buffer), format)
            .map_err(|e| AttachmentError::ImageProcessingFailed(format!("图片编码失败: {}", e)))?;

        let new_size = output_buffer.len() as u64;

        // 重新上传并保持原有的加密策略
        let (upload_stream, new_nonce, is_encrypted) = if old_meta.encrypted {
            let (s, n) =
                crypto.encrypt_streaming(create_mock_stream(output_buffer, new_size as usize))?;
            (s, n, true)
        } else {
            (
                create_mock_stream(output_buffer, new_size as usize),
                vec![],
                false,
            )
        };

        // 通过 store 上传
        let new_etag = store
            .upload_attachment(
                id,
                &attachment_id,
                new_size,
                &old_meta.mimetype,
                upload_stream,
            )
            .await
            .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;

        // 更新元数据
        let mut new_meta = old_meta.clone();
        new_meta.size = new_size;
        new_meta.nonce = new_nonce;
        new_meta.encrypted = is_encrypted;
        new_meta.etag = Some(new_etag);

        update_diary_attachment_locked(&cache, &crypto, &*store, id, new_meta.clone(), &guard)
            .await
            .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;

        let url = state.attachment_url(id, &new_meta.id);
        Ok((new_meta, url))
    };

    match logic.await {
        Ok((meta, url)) => {
            let _ = event.send(AttachmentProcessEvent::Completed(meta, url));
        }
        Err(e) => {
            let _ = event.send(AttachmentProcessEvent::Error(e.to_string()));
        }
    }
}

pub async fn caching_attachment(
    store: &dyn DiaryStore,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    attachment_id: &str,
) {
    let _ = event.send(AttachmentProcessEvent::Started);
    let logic = async {
        let progress_event = event.clone();
        store
            .cache_attachment(
                id,
                attachment_id,
                Arc::new(move |progress| {
                    let _ = progress_event.send(AttachmentProcessEvent::Progress(progress));
                }),
            )
            .await?;
        Ok::<(), AttachmentError>(())
    };
    match logic.await {
        Ok(()) => {
            let _ = event.send(AttachmentProcessEvent::CompletedWithoutData);
        }
        Err(e) => {
            let _ = event.send(AttachmentProcessEvent::Error(e.to_string()));
        }
    }
}

pub async fn save_decrypt_attachment(
    state: &AppState,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    attachment_id: String,
    attachment: AttachmentMeta,
    mut file: File,
) {
    let (crypto, store) = (state.crypto(), state.diary_store());
    let event_res_clone = event.clone();
    let _ = event.send(AttachmentProcessEvent::Started);
    let logic = async move {
        let stream = store
            .download_attachment(id, &attachment_id, None, attachment.etag.as_deref())
            .await?;
        let event_clone = event.clone();
        let stream = tracker_stream(attachment.size, stream, move |p| {
            let _ = event_clone.send(AttachmentProcessEvent::Progress(p));
        });

        let mut stream = if attachment.encrypted {
            crypto.decrypt_streaming(stream, &attachment.nonce, 0)?
        } else {
            stream
        };

        while let Some(chunk) = stream.next().await {
            if let Ok(chunk) = chunk {
                file.write_all(&chunk).map_err(|e| {
                    AttachmentError::FileOperationFailed(format!("写入文件失败:{}", e))
                })?;
            } else {
                return Err(AttachmentError::FileOperationFailed(
                    "下载文件失败".to_string(),
                ));
            }
        }
        Ok::<(), AttachmentError>(())
    };
    match logic.await {
        Err(e) => {
            let _ = event_res_clone.send(AttachmentProcessEvent::Error(e.to_string()));
        }
        Ok(_) => {
            let _ = event_res_clone.send(AttachmentProcessEvent::CompletedWithoutData);
        }
    }
}

pub async fn update_attachment_filename(
    state: &AppState,
    id: &str,
    attachment_id: String,
    new_filename: String,
) -> Result<(), AttachmentError> {
    let cache = state.diary_cache();
    let crypto = state.crypto();
    let store = state.diary_store();
    let diary = get_diary(&cache, &crypto, &*store, id)
        .await
        .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;

    // 确认存在那个附件
    if !diary.attachments.iter().any(|a| a.id == attachment_id) {
        return Err(AttachmentError::InvalidOperation(
            "原附件不存在".to_string(),
        ));
    }

    // filename 仅为展示元数据，物理对象 key 始终保持 attachment ID 不变。
    update_diary_attachment_filename(state, id, attachment_id, new_filename)
        .await
        .map_err(|e| AttachmentError::InvalidOperation(e.to_string()))?;

    Ok(())
}
