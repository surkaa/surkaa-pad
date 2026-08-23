use crate::attachments::attachment::{
    add_attachment_cancelable, caching_attachment, delete_attachment,
    rotate_image_attachment_cancelable, save_decrypt_attachment,
    toggle_attachment_encryption_cancelable, update_attachment_filename,
};
use crate::attachments::attachment_types::AttachmentProcessEvent;
use crate::attachments::AudioWaveform;
use crate::attachments::SharedAttachmentSource;
use crate::diaries::{get_diary, update_diary_attachment_audio_info};
use crate::error::AppError;
use crate::object::STREAM_MIME_TYPE;
use crate::state::AppState;
use crate::stream::{create_mock_stream, file_to_stream};
#[cfg(target_os = "android")]
use crate::utils::id_generate::generate_descending_id;
use crate::utils::{detect_file_mimetype, file_mimetype, file_size};
use std::str::FromStr;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};

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
    Ok(task_pool.spawn_cancelable(move |cancellation| async move {
        add_attachment_cancelable(
            &state,
            Arc::new(event),
            &id,
            encrypted,
            size,
            mimetype,
            stream,
            original_filename,
            &cancellation,
        )
        .await;
    }))
}

/// 将 Android 系统分享的 ContentProvider URI 添加为日记附件。
///
/// 分享来源提供的 MIME 与大小用于兼容不可 seek、无法通过文件描述符取得长度的
/// ContentProvider；文件描述符本身能提供长度时仍以实际长度为准。
/// # Arguments
/// * `event` - 接收上传进度与结果事件的通道
/// * `id` - 日记 ID
/// * `encrypted` - 是否需要加密
/// * `source` - ContentProvider URI、展示文件名、大小与 MIME 类型
/// # Returns
/// * `Result<String, AppError>` - 后台上传任务令牌，可用于取消任务
#[tauri::command]
#[specta::specta]
pub fn cmd_add_shared_attachment(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    encrypted: bool,
    source: SharedAttachmentSource,
) -> Result<String, AppError> {
    let fp = FilePath::from_str(&source.uri).map_err(|e| AppError {
        error_type: "io".into(),
        message: format!("无效的分享文件 URI: {e}"),
    })?;
    let mut option = OpenOptions::new();
    option.read(true);
    let file = app_handle
        .fs()
        .open(fp.clone(), option)
        .map_err(|e| AppError {
            error_type: "io".into(),
            message: format!("无法打开分享文件: {e}"),
        })?;
    // 某些 ContentProvider 返回的是管道描述符，fstat 可能无法取得有效长度；
    // 此时回退到 OpenableColumns.SIZE，而不是先把整个文件收集进内存。
    let size = shared_attachment_size(file_size(&file).unwrap_or(0), source.declared_size)?;
    let mimetype = match normalized_shared_mimetype(source.declared_mimetype) {
        Some(mimetype) => mimetype,
        None => {
            let mut option = OpenOptions::new();
            option.read(true);
            let mut probe = app_handle.fs().open(fp, option).map_err(|e| AppError {
                error_type: "io".into(),
                message: format!("无法读取分享文件类型: {e}"),
            })?;
            detect_file_mimetype(&mut probe)?
        }
    };

    let stream = file_to_stream(file);
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn_cancelable(move |cancellation| async move {
        add_attachment_cancelable(
            &state,
            Arc::new(event),
            &id,
            encrypted,
            size,
            mimetype,
            stream,
            Some(source.original_filename),
            &cancellation,
        )
        .await;
    }))
}

fn shared_attachment_size(metadata_size: u64, declared_size: Option<u64>) -> Result<u64, AppError> {
    if metadata_size > 0 {
        return Ok(metadata_size);
    }
    let size = declared_size.ok_or_else(|| AppError {
        error_type: "io".into(),
        message: "分享来源没有提供文件大小，无法进行流式上传".into(),
    })?;
    if size == 0 {
        return Err(AppError {
            error_type: "io".into(),
            message: "分享来源提供了无效的文件大小".into(),
        });
    }
    Ok(size)
}

fn normalized_shared_mimetype(mimetype: Option<String>) -> Option<String> {
    mimetype
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| {
            !value.is_empty()
                && value != "*/*"
                && value != "application/octet-stream"
                && value != "binary/octet-stream"
                && value.contains('/')
        })
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
    Ok(task_pool.spawn_cancelable(move |cancellation| async move {
        add_attachment_cancelable(
            &state,
            Arc::new(event),
            &id,
            encrypted,
            len as u64,
            mimetype,
            stream,
            None,
            &cancellation,
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
    let _storage_guard = state.lock_storage_operation().await;
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
        Ok(task_pool.spawn_cancelable(move |cancellation| async move {
            add_attachment_cancelable(
                &state,
                Arc::new(event),
                &id,
                encrypted,
                len as u64,
                MIMETYPE.to_string(),
                stream,
                Some(format!("Photo_{}.jpg", generate_descending_id())),
                &cancellation,
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
    Ok(task_pool.spawn_cancelable(move |cancellation| async move {
        let _storage_guard = state.lock_storage_operation().await;
        toggle_attachment_encryption_cancelable(
            &state,
            Arc::new(event),
            &id,
            attachment_id,
            &cancellation,
        )
        .await;
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
    Ok(task_pool.spawn_cancelable(move |cancellation| async move {
        let _storage_guard = state.lock_storage_operation().await;
        rotate_image_attachment_cancelable(
            &state,
            Arc::new(event),
            &id,
            attachment_id,
            rotation,
            &cancellation,
        )
        .await;
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
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn(async move {
        let _storage_guard = state.lock_storage_operation().await;
        let store = state.diary_store();
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
    let attachment = {
        let _storage_guard = state.lock_storage_operation().await;
        let store = state.diary_store();
        let diary = get_diary(&state.diary_cache(), &state.crypto(), &*store, &id).await?;
        diary
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .ok_or_else(|| AppError {
                error_type: "attachment".into(),
                message: "附件不存在".into(),
            })?
            .clone()
    };

    let filepath = app_handle
        .dialog()
        .file()
        // filename 是上传时保留的原始展示文件名，保存时不再根据 MIME
        // 猜测扩展名，避免重复扩展名以及 text/plain 被误判为 .eot。
        .set_file_name(&attachment.filename)
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
        let _storage_guard = state.lock_storage_operation().await;
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
    let _storage_guard = state.lock_storage_operation().await;
    Ok(update_attachment_filename(&state, &id, attachment_id, new_filename).await?)
}

/// 静默保存音频时长和音波，不改变日记的用户编辑时间。
/// # Arguments
/// * `id` - 日记 ID
/// * `attachment_id` - 音频附件 ID
/// * `duration_ms` - 音频时长（毫秒）
/// * `waveform` - 归一化后的紧凑音波数据
/// # Returns
/// * `Result<AttachmentMeta, AppError>` - 返回更新后的附件元数据
#[tauri::command]
#[specta::specta]
pub async fn cmd_update_attachment_audio_info(
    state: State<'_, AppState>,
    id: String,
    attachment_id: String,
    duration_ms: f64,
    waveform: AudioWaveform,
) -> Result<crate::attachments::AttachmentMeta, AppError> {
    if !duration_ms.is_finite() || duration_ms < 0.0 || duration_ms > u64::MAX as f64 {
        return Err(AppError {
            error_type: "attachment".into(),
            message: "音频时长无效".into(),
        });
    }
    let _storage_guard = state.lock_storage_operation().await;
    let store = state.diary_store();
    Ok(update_diary_attachment_audio_info(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        &id,
        &attachment_id,
        duration_ms.round() as u64,
        waveform,
    )
    .await?)
}

#[cfg(test)]
mod shared_attachment_tests {
    use super::{normalized_shared_mimetype, shared_attachment_size};

    #[test]
    fn actual_descriptor_size_takes_precedence() {
        assert_eq!(shared_attachment_size(42, Some(100)).unwrap(), 42);
    }

    #[test]
    fn declared_size_is_used_for_content_provider_without_descriptor_length() {
        assert_eq!(shared_attachment_size(0, Some(1024)).unwrap(), 1024);
    }

    #[test]
    fn invalid_or_missing_declared_size_is_rejected() {
        for size in [None, Some(0)] {
            assert!(shared_attachment_size(0, size).is_err());
        }
    }

    #[test]
    fn provider_mimetype_is_normalized_and_wildcards_are_ignored() {
        assert_eq!(
            normalized_shared_mimetype(Some(" Audio/MP4 ".into())).as_deref(),
            Some("audio/mp4")
        );
        assert_eq!(normalized_shared_mimetype(Some("*/*".into())), None);
        assert_eq!(
            normalized_shared_mimetype(Some("application/octet-stream".into())),
            None
        );
        assert_eq!(normalized_shared_mimetype(Some("unknown".into())), None);
    }
}
