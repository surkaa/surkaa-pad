use crate::attachments::attachment::{deduplicate_filename, generate_attachment_id};
use crate::attachments::attachment_types::{
    ChunkedUploadChunkResult, ChunkedUploadFinishResult, ChunkedUploadStartResult,
};
use crate::attachments::chunked_upload::ChunkedUploadState;
use crate::attachments::AttachmentMeta;
use crate::cryptos::crypto_types::EncryptionAlgorithm::Ctr;
use crate::diaries::{get_diary, update_diary_attachment};
use crate::error::AppError;
use crate::object::STREAM_MIME_TYPE;
use crate::state::AppState;
use crate::utils::id_generate::generate_descending_id;
use aes::cipher::StreamCipher;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::State;
use tauri_plugin_log::log;

/// 初始化分片上传
/// # Arguments
/// * `id` - 日记 ID
/// * `filename` - 附件展示文件名
/// * `mimetype` - MIME 类型，空字符串时根据文件名推断
/// * `encrypted` - 是否在写入前流式加密
/// * `total_size` - 附件总字节数
/// # Returns
/// * `Result<ChunkedUploadStartResult, AppError>` - 上传会话令牌、附件 ID、去重后的文件名和加密 nonce
#[tauri::command]
#[specta::specta]
pub async fn cmd_start_chunked_upload(
    state: State<'_, AppState>,
    id: String,
    filename: String,
    mimetype: String,
    encrypted: bool,
    total_size: f64,
) -> Result<ChunkedUploadStartResult, AppError> {
    if !total_size.is_finite() || total_size < 0.0 || total_size > u64::MAX as f64 {
        return Err(AppError {
            error_type: "chunked_upload".into(),
            message: "附件大小无效".into(),
        });
    }
    let total_size = total_size as u64;
    let mimetype = if mimetype.is_empty() {
        infer::get_from_path(&filename)
            .ok()
            .flatten()
            .map(|t| t.mime_type().to_string())
            .unwrap_or(STREAM_MIME_TYPE.to_string())
    } else {
        mimetype
    };

    let storage_mode_guard = state.lock_storage_operation().await;
    let (crypto, cache, store) = (state.crypto(), state.diary_cache(), state.diary_store());

    let attachment_id = generate_attachment_id()?;
    let (cipher, nonce) = if encrypted {
        let (cipher, nonce) = crypto.create_ctr_cipher()?;
        (Some(cipher), Some(nonce))
    } else {
        (None, None)
    };
    let diary = get_diary(&cache, &crypto, &*store, &id)
        .await
        .map_err(|e| AppError {
            error_type: "attachment".into(),
            message: e.to_string(),
        })?;
    let existing_filenames: HashSet<String> = diary
        .attachments
        .iter()
        .map(|attachment| attachment.filename.clone())
        .collect();
    let filename_allocator = state
        .filename_allocators()
        .entry(id.clone())
        .or_default()
        .clone();
    let filename = {
        let mut pending_filenames = filename_allocator.lock().await;
        let combined = existing_filenames
            .union(&pending_filenames)
            .cloned()
            .collect();
        let filename = deduplicate_filename(&filename, &combined);
        pending_filenames.insert(filename.clone());
        filename
    };
    let session = match store
        .begin_attachment_upload(&id, &attachment_id, total_size, &mimetype)
        .await
    {
        Ok(session) => session,
        Err(error) => {
            filename_allocator.lock().await.remove(&filename);
            return Err(AppError {
                error_type: "chunked_upload".into(),
                message: error.to_string(),
            });
        }
    };

    let upload_token = generate_descending_id();
    let upload_state = ChunkedUploadState {
        diary_id: id,
        attachment_id: attachment_id.clone(),
        filename: filename.clone(),
        mimetype,
        encrypted,
        nonce: nonce.clone().unwrap_or_default(),
        cipher,
        session: Some(session),
        store,
        _storage_mode_guard: storage_mode_guard,
        total_size,
        uploaded_bytes: 0,
        next_part_number: 1,
    };

    state.chunked_uploads().insert(
        upload_token.clone(),
        Arc::new(tokio::sync::Mutex::new(upload_state)),
    );

    Ok(ChunkedUploadStartResult {
        upload_token,
        attachment_id,
        filename,
        nonce,
    })
}

/// 上传单个分片
/// # Arguments
/// * `upload_token` - 初始化分片上传时返回的会话令牌
/// * `chunk_index` - 从 0 开始的分片索引，必须按顺序上传
/// * `data` - 当前分片的原始字节
/// # Returns
/// * `Result<ChunkedUploadChunkResult, AppError>` - 分片编号、ETag 以及已上传/总字节数
#[tauri::command]
#[specta::specta]
pub async fn cmd_upload_chunk(
    state: State<'_, AppState>,
    upload_token: String,
    chunk_index: u32,
    data: Vec<u8>,
) -> Result<ChunkedUploadChunkResult, AppError> {
    let uploads = state.chunked_uploads();
    let upload = uploads
        .get(&upload_token)
        .map(|entry| entry.value().clone())
        .ok_or_else(|| AppError {
            error_type: "chunked_upload".into(),
            message: "分片上传会话不存在".into(),
        })?;
    let mut upload = upload.lock().await;

    if chunk_index != upload.next_part_number - 1 {
        return Err(AppError {
            error_type: "chunked_upload".into(),
            message: format!(
                "分片顺序错误：期望 {}，收到 {}",
                upload.next_part_number - 1,
                chunk_index
            ),
        });
    }

    let part_number = upload.next_part_number;
    let mut upload_data = data;
    if upload.encrypted {
        if let Some(cipher) = upload.cipher.as_mut() {
            cipher.apply_keystream(&mut upload_data);
        }
    }

    let data_len = upload_data.len() as u64;
    let chunk_result = upload
        .session
        .as_mut()
        .ok_or_else(|| AppError {
            error_type: "chunked_upload".into(),
            message: "分片上传会话已经结束".into(),
        })?
        .write_chunk(upload_data)
        .await;
    let chunk = match chunk_result {
        Ok(chunk) => chunk,
        Err(error) => {
            let session = upload.session.take();
            let diary_id = upload.diary_id.clone();
            let filename = upload.filename.clone();
            drop(upload);
            uploads.remove(&upload_token);
            if let Some(session) = session {
                let _ = session.abort().await;
            }
            release_filename(&state, diary_id, &filename).await;
            return Err(AppError {
                error_type: "chunked_upload".into(),
                message: error.to_string(),
            });
        }
    };
    if chunk.0 != part_number {
        let session = upload.session.take();
        let diary_id = upload.diary_id.clone();
        let filename = upload.filename.clone();
        drop(upload);
        uploads.remove(&upload_token);
        if let Some(session) = session {
            let _ = session.abort().await;
        }
        release_filename(&state, diary_id, &filename).await;
        return Err(AppError {
            error_type: "chunked_upload".into(),
            message: format!(
                "存储层返回了错误的分片编号：期望 {part_number}，收到 {}",
                chunk.0
            ),
        });
    }

    upload.uploaded_bytes += data_len;
    upload.next_part_number += 1;

    Ok(ChunkedUploadChunkResult {
        part_number,
        etag: chunk.1,
        uploaded_bytes: upload.uploaded_bytes as f64,
        total_bytes: upload.total_size as f64,
    })
}

/// 完成分片上传
/// # Arguments
/// * `upload_token` - 分片上传会话令牌
/// # Returns
/// * `Result<ChunkedUploadFinishResult, AppError>` - 已保存的附件元数据和本地 HTTP URL
#[tauri::command]
#[specta::specta]
pub async fn cmd_finish_chunked_upload(
    state: State<'_, AppState>,
    upload_token: String,
) -> Result<ChunkedUploadFinishResult, AppError> {
    let upload = state
        .chunked_uploads()
        .remove(&upload_token)
        .ok_or_else(|| AppError {
            error_type: "chunked_upload".into(),
            message: "分片上传会话不存在".into(),
        })?
        .1;
    let mut upload = upload.lock().await;
    let (cache, crypto) = (state.diary_cache(), state.crypto());

    let filename_allocator = state
        .filename_allocators()
        .entry(upload.diary_id.clone())
        .or_default()
        .clone();
    let session = upload.session.take().ok_or_else(|| AppError {
        error_type: "chunked_upload".into(),
        message: "分片上传会话已经结束".into(),
    })?;
    let finish_result = session.finish().await;
    filename_allocator.lock().await.remove(&upload.filename);
    let etag = finish_result.map_err(|error| AppError {
        error_type: "chunked_upload".into(),
        message: error.to_string(),
    })?;
    log::info!(
        "[chunked_upload] complete: diary_id={}, attachment_id={}, etag={}, total_size={}, uploaded_bytes={}",
        upload.diary_id,
        upload.attachment_id,
        etag,
        upload.total_size,
        upload.uploaded_bytes
    );

    let attachment = AttachmentMeta {
        id: upload.attachment_id.clone(),
        filename: upload.filename.clone(),
        mimetype: upload.mimetype.clone(),
        size: if upload.total_size > 0 {
            upload.total_size
        } else {
            upload.uploaded_bytes
        },
        nonce: upload.nonce.clone(),
        encrypted: upload.encrypted,
        algorithm: Ctr,
        etag: Some(etag),
    };

    let manifest_result = update_diary_attachment(
        &cache,
        &crypto,
        &*upload.store,
        &upload.diary_id,
        attachment.clone(),
    )
    .await;
    if let Err(error) = manifest_result {
        let rollback = upload
            .store
            .delete_attachment(&upload.diary_id, &attachment.id)
            .await;
        let message = match rollback {
            Ok(()) => error.to_string(),
            Err(rollback_error) => format!("{error}；附件对象回滚失败：{rollback_error}"),
        };
        return Err(AppError {
            error_type: "attachment".into(),
            message,
        });
    }

    let url = state.attachment_url(&upload.diary_id, &attachment.id);
    Ok(ChunkedUploadFinishResult { attachment, url })
}

/// 取消分片上传
/// # Arguments
/// * `upload_token` - 分片上传会话令牌
/// # Returns
/// * `Result<(), AppError>` - 成功时已取消远程 multipart 并删除本地临时文件
#[tauri::command]
#[specta::specta]
pub async fn cmd_abort_chunked_upload(
    state: State<'_, AppState>,
    upload_token: String,
) -> Result<(), AppError> {
    let upload = state
        .chunked_uploads()
        .remove(&upload_token)
        .ok_or_else(|| AppError {
            error_type: "chunked_upload".into(),
            message: "分片上传会话不存在".into(),
        })?
        .1;
    let mut upload = upload.lock().await;
    let session = upload.session.take().ok_or_else(|| AppError {
        error_type: "chunked_upload".into(),
        message: "分片上传会话已经结束".into(),
    })?;
    let abort_result = session.abort().await;
    release_filename(&state, upload.diary_id.clone(), &upload.filename).await;
    abort_result.map_err(|error| AppError {
        error_type: "chunked_upload".into(),
        message: error.to_string(),
    })
}

async fn release_filename(state: &AppState, diary_id: String, filename: &str) {
    let filename_allocator = state
        .filename_allocators()
        .entry(diary_id)
        .or_default()
        .clone();
    filename_allocator.lock().await.remove(filename);
}
