use crate::attachments::attachment::{
    add_attachment, caching_attachment, delete_attachment, rotate_image_attachment,
    save_decrypt_attachment, toggle_attachment_encryption, update_attachment_filename,
    DIARY_ALLOCATORS,
};
use crate::attachments::attachment_types::{
    AttachmentProcessEvent, ChunkedUploadChunkResult, ChunkedUploadFinishResult,
    ChunkedUploadStartResult,
};
use crate::attachments::chunked_upload::ChunkedUploadState;
use crate::attachments::{get_full_attachment_url, AttachmentMeta};
use crate::cryptos::crypto_types::EncryptionAlgorithm::Ctr;
use crate::diaries::{get_diary, update_diary_attachment};
use crate::error::AppError;
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

/// 给日记添加附件
/// # Arguments
/// * `id` - 日记 ID
/// * `access_str` - 文件访问路径。
/// * `encrypted` - 是否需要加密
/// # Returns
/// * `Result<String, String>` - 成功时返回取消Token，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_add_attachment(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    access_str: String,
    encrypted: bool,
) -> Result<String, AppError> {
    let fp = FilePath::from_str(&access_str)
        .map_err(|e| AppError { error_type: "io".into(), message: format!("无效的文件路径: {}", e) })?;
    let mut option = OpenOptions::new();
    option.read(true);
    let file = app_handle
        .fs()
        .open(fp, option)
        .map_err(|e| AppError { error_type: "io".into(), message: format!("无法打开文件: {}", e) })?;
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
        )
        .await;
    })?)
}

/// 直接传字节数据给日记添加附件
/// # Arguments
/// * `id` - 日记 ID
/// * `data` - 文件字节数据
/// * `mimetype` - 附件 MIME 类型
/// * `encrypted` - 是否需要加密
/// # Returns
/// * `Result<String, String>` - 成功时返回取消Token，失败时返回错误信息
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
            .unwrap_or("application/octet-stream".to_string())
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
        )
        .await;
    })?)
}

/// 删除日记的附件
/// # Arguments
/// * `id` - 日记 ID
/// * `filename` - 附件 ID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_delete_attachment(
    state: State<'_, AppState>,
    id: &str,
    filename: String,
) -> Result<(), AppError> {
    let client = state.oss_client();
    Ok(delete_attachment(
        &state.diary_cache(),
        &state.local_file_cache(),
        &state.crypto(),
        &client,
        id,
        filename,
    )
    .await?)
}

/// 拍摄图片来添加
/// # Arguments
/// * `id` - 日记 ID
/// * `encrypted` - 是否需要加密
/// # Returns
/// * `Result<String, String>` - 成功时返回取消Token，失败时返回错误信息
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

        let result = app
            .native_camera()
            .take_picture()
            .map_err(|e| AppError { error_type: "camera".into(), message: e.to_string() })?;
        let base64_data = result.image_data;
        let binary_data = STANDARD
            .decode(base64_data)
            .map_err(|e| AppError { error_type: "base64".into(), message: e.to_string() })?;
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
            )
            .await;
        })?)
    }
    #[cfg(not(target_os = "android"))]
    {
        // 简单使用一下参数避免编译器警告
        let _ = (app, state, event, id, encrypted);
        Err(AppError { error_type: "platform".into(), message: "拍照功能仅在 Android 上可用".into() })
    }
}

/// 将加密的附件转成未加密的、将未加密的附件转成加密的
/// # Arguments
/// * `id` - 日记 ID
/// * `filename` - 附件 ID
/// # Returns
/// * `Result<String, String>` - 成功时返回取消Token，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_toggle_attachment_encryption(
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    filename: String,
) -> Result<String, AppError> {
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn(async move {
        toggle_attachment_encryption(&state, Arc::new(event), &id, filename).await;
    })?)
}

/// 旋转图片附件 顺时针90度、逆时针90度和180度
/// # Arguments
/// * `id` - 日记 ID
/// * `filename` - 附件 ID
/// * `rotation` - 旋转角度，单位为度，支持90、-90和180
/// # Returns
/// * `Result<String, String>` - 成功时返回取消Token，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_rotate_image_attachment(
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    filename: String,
    rotation: i32,
) -> Result<String, AppError> {
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn(async move {
        rotate_image_attachment(&state, Arc::new(event), &id, filename, rotation).await;
    })?)
}

/// 主动缓存云端附件到本地
/// # Arguments
/// * `id` - 日记 ID
/// * `filename` - 附件 ID
/// # Returns
/// * `Result<String, String>` - 成功时返回取消Token，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_caching_attachment(
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    filename: String,
) -> Result<String, AppError> {
    let lfc = state.local_file_cache();
    let client = state.oss_client();
    Ok(state.task_pool().spawn(async move {
        caching_attachment(&lfc, &client, Arc::new(event), &id, &filename).await;
    })?)
}

/// 让用户选择一个位置保存附近明文
/// # Arguments
/// * `id` - 日记 ID
/// * `filename` - 附件 ID
/// # Returns
/// * `Result<String, String>` - 成功时返回取消Token，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_save_decrypt_attachment(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    filename: String,
) -> Result<String, AppError> {
    let diary = get_diary(&state.diary_cache(), &state.local_file_cache(), &state.crypto(), &state.oss_client(), &id).await?;
    let attachment = diary
        .attachments
        .iter()
        .find(|a| a.filename == filename)
        .ok_or_else(|| AppError { error_type: "attachment".into(), message: "附件不存在".into() })?
        .clone();

    let ext = infer::get_from_mime(&attachment.mimetype)
        .map(|t| t.extension())
        .unwrap_or("");

    let filepath = app_handle
        .dialog()
        .file()
        .set_file_name(format!("{}.{}", attachment.filename, ext))
        .blocking_save_file()
        .ok_or_else(|| AppError { error_type: "user".into(), message: "未选择".into() })?;

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
            filename,
            attachment,
            file,
        )
        .await;
    })?)
}

/// 重命名附件
/// # Arguments
/// * `id` - 日记 ID
/// * `old_filename` - 旧附件 ID
/// * `new_filename` - 新附件 ID
/// * `new_content`  - 新的完整内容
/// # Returns
/// * `Result<(), String>` - 成功时返回null，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_update_attachment_filename(
    state: State<'_, AppState>,
    id: String,
    old_filename: String,
    new_filename: String,
    new_content: String,
) -> Result<(), AppError> {
    Ok(update_attachment_filename(
        &state,
        &id,
        old_filename,
        new_filename,
        new_content
    ).await?)
}

/// 初始化分片上传
#[tauri::command]
#[specta::specta]
pub async fn cmd_start_chunked_upload(
    state: State<'_, AppState>,
    id: String,
    filename: String,
    mimetype: String,
    encrypted: bool,
    total_size: u64,
) -> Result<ChunkedUploadStartResult, AppError> {
    let mimetype = if mimetype.is_empty() {
        infer::get_from_path(&filename)
            .ok()
            .flatten()
            .map(|t| t.mime_type().to_string())
            .unwrap_or("application/octet-stream".to_string())
    } else {
        mimetype
    };

    let (crypto, cache, lfc, client) = (
        state.crypto(),
        state.diary_cache(),
        state.local_file_cache(),
        state.oss_client(),
    );

    // MEX 算法分配附件 ID
    let alloc_state = DIARY_ALLOCATORS.entry(id.clone()).or_default().clone();
    let allocated_id = {
        let mut pending_ids = alloc_state.lock().await;
        let diary = get_diary(&cache, &lfc, &crypto, &client, &id)
            .await
            .map_err(|e| AppError {
                error_type: "attachment".into(),
                message: e.to_string(),
            })?;

        let existing_ids: HashSet<u32> = diary
            .attachments
            .iter()
            .filter_map(|att| att.filename.parse::<u32>().ok())
            .collect();

        let new_id = (1..).find(|i| !existing_ids.contains(i) && !pending_ids.contains(i));
        let new_id = new_id.ok_or_else(|| AppError {
            error_type: "attachment".into(),
            message: "无法分配附件 ID".into(),
        })?;

        pending_ids.insert(new_id);
        new_id
    };
    let attachment_filename = allocated_id.to_string();
    let key = remote_attachments_key(&id, &attachment_filename);

    // 创建加密 cipher（如需要）
    let (cipher, nonce) = if encrypted {
        let (c, n) = crypto.create_ctr_cipher()?;
        (Some(std::sync::Mutex::new(c)), Some(n))
    } else {
        (None, None)
    };

    // 初始化 S3 分片上传
    let upload_id = client
        .initiate_multipart_upload(&key, &mimetype)
        .await
        .map_err(|e| AppError {
            error_type: "oss".into(),
            message: e.to_string(),
        })?;

    // 创建本地文件缓存句柄
    let lfc_handle = lfc.begin_chunked_save(&key).await.map_err(|e| AppError {
        error_type: "cache".into(),
        message: e.to_string(),
    })?;

    // 生成 upload token
    let upload_token = generate_descending_id();

    // 存储分片上传状态
    let upload_state = ChunkedUploadState {
        diary_id: id,
        allocated_id,
        upload_id,
        key,
        filename: attachment_filename.clone(),
        mimetype,
        encrypted,
        nonce: nonce.clone().unwrap_or_default(),
        cipher,
        parts: Vec::new(),
        lfc_handle,
        total_size,
        uploaded_bytes: 0,
        next_part_number: 1,
    };

    state.chunked_uploads().insert(upload_token.clone(), upload_state);

    Ok(ChunkedUploadStartResult {
        upload_token,
        attachment_filename,
        nonce,
    })
}

/// 上传单个分片
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

    // 上传分片到 S3
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

    let (cache, lfc, crypto, client) = (
        state.diary_cache(),
        state.local_file_cache(),
        state.crypto(),
        state.oss_client(),
    );

    // 完成 S3 分片上传
    let etag = client
        .complete_multipart_upload(&upload.key, &upload.upload_id, upload.parts)
        .await
        .map_err(|e| AppError {
            error_type: "oss".into(),
            message: e.to_string(),
        })?;

    // 固化本地缓存
    let _ = upload.lfc_handle.finalize(&etag).await;

    // 释放 MEX 占位
    let alloc_state = DIARY_ALLOCATORS
        .entry(upload.diary_id.clone())
        .or_default()
        .clone();
    {
        let mut pending_ids = alloc_state.lock().await;
        pending_ids.remove(&upload.allocated_id);
    }

    // 构建附件元数据
    let attachment = AttachmentMeta {
        filename: upload.filename.clone(),
        mimetype: upload.mimetype,
        size: upload.total_size,
        nonce: upload.nonce,
        encrypted: upload.encrypted,
        algorithm: Ctr,
        etag: Some(etag),
    };

    // 更新日记 manifest
    update_diary_attachment(&cache, &lfc, &crypto, &client, &upload.diary_id, attachment.clone())
        .await
        .map_err(|e| AppError {
            error_type: "attachment".into(),
            message: e.to_string(),
        })?;

    // 获取附件 URL
    let url = get_full_attachment_url(&upload.diary_id, &attachment, &client)
        .await
        .map_err(|e| AppError {
            error_type: "oss".into(),
            message: e.to_string(),
        })?;

    Ok(ChunkedUploadFinishResult { attachment, url })
}

/// 取消分片上传
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

    // 取消 S3 分片上传
    let _ = state
        .oss_client()
        .abort_multipart_upload(&upload.key, &upload.upload_id)
        .await;

    // 删除本地临时文件
    upload.lfc_handle.abort().await;

    // 释放 MEX 占位
    let alloc_state = DIARY_ALLOCATORS
        .entry(upload.diary_id)
        .or_default()
        .clone();
    {
        let mut pending_ids = alloc_state.lock().await;
        pending_ids.remove(&upload.allocated_id);
    }

    Ok(())
}