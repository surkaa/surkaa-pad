use crate::attachments::attachment::{
    add_attachment, caching_attachment, deduplicate_filename, delete_attachment,
    generate_attachment_id, rotate_image_attachment, save_decrypt_attachment,
    toggle_attachment_encryption, update_attachment_filename,
};
use crate::attachments::attachment_types::{
    AttachmentProcessEvent, ChunkedUploadChunkResult, ChunkedUploadFinishResult,
    ChunkedUploadStartResult,
};
use crate::attachments::chunked_upload::ChunkedUploadState;
use crate::attachments::AttachmentMeta;
use crate::cryptos::crypto_types::EncryptionAlgorithm::Ctr;
use crate::diaries::{get_diary, update_diary_attachment};
use crate::error::AppError;
use crate::object::STREAM_MIME_TYPE;
use crate::state::AppState;
use crate::storages::remote_attachments_key;
use crate::stream::{create_mock_stream, file_to_stream};
use crate::utils::id_generate::generate_descending_id;
use crate::utils::{file_mimetype, file_size};
use aes::cipher::StreamCipher;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
use tauri_plugin_log::log;

/// 给日记添加附件
/// # Arguments
/// * `event` - 接收上传进度与结果事件的通道
/// * `id` - 日记 ID
/// * `access_str` - Tauri 文件系统可访问的文件路径
/// * `encrypted` - 是否需要加密
/// * `original_filename` - 附件展示文件名，未提供时使用默认名称
/// # Returns
/// * `Result<String, AppError>` - 后台上传任务令牌，可用于取消任务
#[tauri::command]
#[specta::specta]
pub fn cmd_add_attachment(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    access_str: String,
    encrypted: bool,
    original_filename: Option<String>,
) -> Result<String, AppError> {
    let fp = FilePath::from_str(&access_str).map_err(|e| AppError {
        error_type: "io".into(),
        message: format!("无效的文件路径: {}", e),
    })?;
    let mut option = OpenOptions::new();
    option.read(true);
    let file = app_handle.fs().open(fp, option).map_err(|e| AppError {
        error_type: "io".into(),
        message: format!("无法打开文件: {}", e),
    })?;
    let size = file_size(&file)?;
    let (mimetype, file) = file_mimetype(file)?;
    let stream = file_to_stream(file);
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn(async move {
        add_attachment(
            &state,
            Arc::new(event),
            &id,
            encrypted,
            size,
            mimetype,
            stream,
            original_filename,
        )
        .await;
    }))
}

/// 直接传字节数据给日记添加附件
/// # Arguments
/// * `event` - 接收上传进度与结果事件的通道
/// * `id` - 日记 ID
/// * `data` - 文件字节数据
/// * `mimetype` - 附件 MIME 类型
/// * `encrypted` - 是否需要加密
/// # Returns
/// * `Result<String, AppError>` - 后台上传任务令牌，可用于取消任务
#[tauri::command]
#[specta::specta]
pub fn cmd_add_attachment_memory(
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    data: Vec<u8>,
    mimetype: String,
    encrypted: bool,
) -> Result<String, AppError> {
    let len = data.len();
    let mimetype = if mimetype.is_empty() {
        let end = std::cmp::min(data.len(), 128);
        infer::get(&data[..end])
            .map(|t| t.mime_type().to_string())
            .unwrap_or(STREAM_MIME_TYPE.to_string())
    } else {
        mimetype
    };
    let stream = create_mock_stream(data, len);
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn(async move {
        add_attachment(
            &state,
            Arc::new(event),
            &id,
            encrypted,
            len as u64,
            mimetype,
            stream,
            None,
        )
        .await;
    }))
}

/// 删除日记的附件
/// # Arguments
/// * `id` - 日记 ID
/// * `attachment_id` - 附件 ID
/// # Returns
/// * `Result<(), AppError>` - 成功时已删除附件引用和存储对象
#[tauri::command]
#[specta::specta]
pub async fn cmd_delete_attachment(
    state: State<'_, AppState>,
    id: &str,
    attachment_id: String,
) -> Result<(), AppError> {
    let store = state.diary_store();
    Ok(delete_attachment(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        id,
        attachment_id,
    )
    .await?)
}

/// 拍摄图片来添加
/// # Arguments
/// * `event` - 接收上传进度与结果事件的通道
/// * `id` - 日记 ID
/// * `encrypted` - 是否需要加密
/// # Returns
/// * `Result<String, AppError>` - Android 上返回后台上传任务令牌，其他平台返回不支持错误
#[tauri::command]
#[specta::specta]
pub async fn cmd_add_image_attachment_from_camera(
    app: AppHandle,
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    encrypted: bool,
) -> Result<String, AppError> {
    #[cfg(target_os = "android")]
    {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        use tauri_plugin_native_camera::NativeCameraExt;
        const MIMETYPE: &str = "image/jpeg";

        let result = app.native_camera().take_picture().map_err(|e| AppError {
            error_type: "camera".into(),
            message: e.to_string(),
        })?;
        let base64_data = result.image_data;
        let binary_data = STANDARD.decode(base64_data).map_err(|e| AppError {
            error_type: "base64".into(),
            message: e.to_string(),
        })?;
        let len = binary_data.len();
        let stream = create_mock_stream(binary_data, len);
        let task_pool = state.task_pool();
        let state = state.inner().clone();
        Ok(task_pool.spawn(async move {
            add_attachment(
                &state,
                Arc::new(event),
                &id,
                encrypted,
                len as u64,
                MIMETYPE.to_string(),
                stream,
                Some(format!("Photo_{}.jpg", generate_descending_id())),
            )
            .await;
        }))
    }
    #[cfg(not(target_os = "android"))]
    {
        // 简单使用一下参数避免编译器警告
        let _ = (app, state, event, id, encrypted);
        Err(AppError {
            error_type: "platform".into(),
            message: "拍照功能仅在 Android 上可用".into(),
        })
    }
}

/// 将加密的附件转成未加密的、将未加密的附件转成加密的
/// # Arguments
/// * `event` - 接收处理进度与结果事件的通道
/// * `id` - 日记 ID
/// * `attachment_id` - 附件 ID
/// # Returns
/// * `Result<String, AppError>` - 后台处理任务令牌，可用于取消任务
#[tauri::command]
#[specta::specta]
pub fn cmd_toggle_attachment_encryption(
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    attachment_id: String,
) -> Result<String, AppError> {
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn(async move {
        toggle_attachment_encryption(&state, Arc::new(event), &id, attachment_id).await;
    }))
}

/// 旋转图片附件 顺时针90度、逆时针90度和180度
/// # Arguments
/// * `event` - 接收处理进度与结果事件的通道
/// * `id` - 日记 ID
/// * `attachment_id` - 附件 ID
/// * `rotation` - 旋转角度，单位为度，支持90、-90和180
/// # Returns
/// * `Result<String, AppError>` - 后台处理任务令牌，可用于取消任务
#[tauri::command]
#[specta::specta]
pub fn cmd_rotate_image_attachment(
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    attachment_id: String,
    rotation: i32,
) -> Result<String, AppError> {
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn(async move {
        rotate_image_attachment(&state, Arc::new(event), &id, attachment_id, rotation).await;
    }))
}

/// 主动缓存云端附件到本地
/// # Arguments
/// * `event` - 接收缓存进度与结果事件的通道
/// * `id` - 日记 ID
/// * `attachment_id` - 附件 ID
/// # Returns
/// * `Result<String, AppError>` - 后台缓存任务令牌，可用于取消任务
#[tauri::command]
#[specta::specta]
pub fn cmd_caching_attachment(
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    attachment_id: String,
) -> Result<String, AppError> {
    let store = state.diary_store();
    Ok(state.task_pool().spawn(async move {
        caching_attachment(&*store, Arc::new(event), &id, &attachment_id).await;
    }))
}

/// 让用户选择一个位置保存附件明文
/// # Arguments
/// * `event` - 接收保存进度与结果事件的通道
/// * `id` - 日记 ID
/// * `attachment_id` - 附件 ID
/// # Returns
/// * `Result<String, AppError>` - 选定保存位置后返回后台保存任务令牌
#[tauri::command]
#[specta::specta]
pub async fn cmd_save_decrypt_attachment(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    attachment_id: String,
) -> Result<String, AppError> {
    let store = state.diary_store();
    let diary = get_diary(&state.diary_cache(), &state.crypto(), &*store, &id).await?;
    let attachment = diary
        .attachments
        .iter()
        .find(|attachment| attachment.id == attachment_id)
        .ok_or_else(|| AppError {
            error_type: "attachment".into(),
            message: "附件不存在".into(),
        })?
        .clone();

    let ext = infer::get_from_mime(&attachment.mimetype)
        .map(|t| t.extension())
        .unwrap_or("");

    let filepath = app_handle
        .dialog()
        .file()
        .set_file_name(format!("{}.{}", attachment.filename, ext))
        .blocking_save_file()
        .ok_or_else(|| AppError {
            error_type: "user".into(),
            message: "未选择".into(),
        })?;

    let mut option = OpenOptions::new();
    option.write(true).truncate(true).create(true);
    let file = app_handle
        .fs()
        .open(filepath, option)
        .map_err(|e| AppError {
            error_type: "io".into(),
            message: e.to_string(),
        })?;

    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn(async move {
        save_decrypt_attachment(
            &state,
            Arc::new(event),
            &id,
            attachment_id,
            attachment,
            file,
        )
        .await;
    }))
}

/// 重命名附件
/// # Arguments
/// * `id` - 日记 ID
/// * `attachment_id` - 附件 ID
/// * `new_filename` - 新的展示文件名
/// # Returns
/// * `Result<(), AppError>` - 成功时已更新 Manifest 中的展示文件名
#[tauri::command]
#[specta::specta]
pub async fn cmd_update_attachment_filename(
    state: State<'_, AppState>,
    id: String,
    attachment_id: String,
    new_filename: String,
) -> Result<(), AppError> {
    Ok(update_attachment_filename(&state, &id, attachment_id, new_filename).await?)
}

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
    let mimetype = if mimetype.is_empty() {
        infer::get_from_path(&filename)
            .ok()
            .flatten()
            .map(|t| t.mime_type().to_string())
            .unwrap_or(STREAM_MIME_TYPE.to_string())
    } else {
        mimetype
    };

    let (crypto, cache, store) = (state.crypto(), state.diary_cache(), state.diary_store());
    let lfc = state.local_file_cache();

    let attachment_id = generate_attachment_id()?;
    // 先完成所有不需要占用 filename 的易失败准备工作。
    let (cipher, nonce) = if encrypted {
        let (cipher, nonce) = crypto.create_ctr_cipher()?;
        (Some(std::sync::Mutex::new(cipher)), Some(nonce))
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
    let key = remote_attachments_key(&id, &attachment_id);

    // 初始化 S3 分片上传（本地模式跳过）
    let upload_id = if state.is_remote_enabled() {
        match state
            .oss_client()
            .initiate_multipart_upload(&key, &mimetype)
            .await
        {
            Ok(upload_id) => upload_id,
            Err(error) => {
                filename_allocator.lock().await.remove(&filename);
                return Err(AppError {
                    error_type: "oss".into(),
                    message: error.to_string(),
                });
            }
        }
    } else {
        String::new()
    };

    // 创建本地文件缓存句柄
    let lfc_handle = match lfc.begin_chunked_save(&key).await {
        Ok(handle) => handle,
        Err(error) => {
            if state.is_remote_enabled() {
                let _ = state
                    .oss_client()
                    .abort_multipart_upload(&key, &upload_id)
                    .await;
            }
            filename_allocator.lock().await.remove(&filename);
            return Err(AppError {
                error_type: "cache".into(),
                message: error.to_string(),
            });
        }
    };

    // 生成 upload token
    let upload_token = generate_descending_id();

    // 存储分片上传状态
    let upload_state = ChunkedUploadState {
        diary_id: id,
        attachment_id: attachment_id.clone(),
        upload_id,
        key,
        filename: filename.clone(),
        mimetype,
        encrypted,
        nonce: nonce.clone().unwrap_or_default(),
        cipher,
        parts: Vec::new(),
        lfc_handle,
        total_size: total_size as u64,
        uploaded_bytes: 0,
        next_part_number: 1,
    };

    state
        .chunked_uploads()
        .insert(upload_token.clone(), upload_state);

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

    // 获取可变引用
    let mut upload = uploads.get_mut(&upload_token).ok_or_else(|| AppError {
        error_type: "chunked_upload".into(),
        message: "分片上传会话不存在".into(),
    })?;

    // 验证分片顺序
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

    // 加密（如需要）
    if upload.encrypted {
        if let Some(ref cipher) = upload.cipher {
            let mut c = cipher.lock().map_err(|_| AppError {
                error_type: "crypto".into(),
                message: "加密锁中毒".into(),
            })?;
            c.apply_keystream(&mut upload_data);
        }
    }

    // 上传分片到 S3（本地模式跳过）
    let etag = if state.is_remote_enabled() {
        let (etag, _) = state
            .oss_client()
            .upload_part(
                &upload.key,
                part_number,
                &upload.upload_id,
                upload_data.clone(),
                &upload.mimetype,
            )
            .await
            .map_err(|e| AppError {
                error_type: "oss".into(),
                message: e.to_string(),
            })?;
        etag
    } else {
        String::new()
    };

    // 写入本地缓存
    upload
        .lfc_handle
        .write_chunk(&upload_data)
        .await
        .map_err(|e| AppError {
            error_type: "cache".into(),
            message: e.to_string(),
        })?;

    // 更新状态
    let data_len = upload_data.len() as u64;
    upload.parts.push((etag.clone(), part_number));
    upload.uploaded_bytes += data_len;
    upload.next_part_number += 1;

    let uploaded_bytes = upload.uploaded_bytes;
    let total_bytes = upload.total_size;

    Ok(ChunkedUploadChunkResult {
        part_number,
        etag,
        uploaded_bytes: uploaded_bytes as f64,
        total_bytes: total_bytes as f64,
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

    let (cache, crypto, store) = (state.diary_cache(), state.crypto(), state.diary_store());

    // 完成 S3 分片上传（本地模式跳过）
    let etag = if state.is_remote_enabled() {
        let parts_count = upload.parts.len();
        let etag = state
            .oss_client()
            .complete_multipart_upload(&upload.key, &upload.upload_id, upload.parts)
            .await
            .map_err(|e| AppError {
                error_type: "oss".into(),
                message: e.to_string(),
            })?;
        log::info!("[chunked_upload] complete: key={}, parts={}, etag={}, total_size={}, uploaded_bytes={}", upload.key, parts_count, etag, upload.total_size, upload.uploaded_bytes);
        etag
    } else {
        log::info!(
            "[chunked_upload] complete (local): key={}, total_size={}, uploaded_bytes={}",
            upload.key,
            upload.total_size,
            upload.uploaded_bytes
        );
        String::new()
    };

    // 固化本地缓存
    let _ = upload.lfc_handle.finalize(&etag).await;

    let filename_allocator = state
        .filename_allocators()
        .entry(upload.diary_id.clone())
        .or_default()
        .clone();
    filename_allocator.lock().await.remove(&upload.filename);

    // 构建附件元数据
    let attachment = AttachmentMeta {
        id: upload.attachment_id,
        filename: upload.filename.clone(),
        mimetype: upload.mimetype,
        size: if upload.total_size > 0 {
            upload.total_size
        } else {
            upload.uploaded_bytes
        },
        nonce: upload.nonce,
        encrypted: upload.encrypted,
        algorithm: Ctr,
        etag: Some(etag),
    };

    // 更新日记 manifest
    update_diary_attachment(
        &cache,
        &crypto,
        &*store,
        &upload.diary_id,
        attachment.clone(),
    )
    .await
    .map_err(|e| AppError {
        error_type: "attachment".into(),
        message: e.to_string(),
    })?;

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

    // 取消 S3 分片上传（本地模式跳过）
    if state.is_remote_enabled() {
        let _ = state
            .oss_client()
            .abort_multipart_upload(&upload.key, &upload.upload_id)
            .await;
    }

    // 删除本地临时文件
    upload.lfc_handle.abort().await;

    let filename_allocator = state
        .filename_allocators()
        .entry(upload.diary_id)
        .or_default()
        .clone();
    filename_allocator.lock().await.remove(&upload.filename);

    Ok(())
}
