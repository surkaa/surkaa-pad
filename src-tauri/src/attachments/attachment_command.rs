use crate::attachments::attachment::{add_attachment, caching_attachment, delete_attachment, rotate_image_attachment, toggle_attachment_encryption};
use crate::attachments::attachment_types::AttachmentProcessEvent;
use crate::state::AppState;
use crate::stream::create_mock_stream;
use crate::utils::{file_mimetype, file_size, file_to_stream, open_access_str_file};
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::State;

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
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    access_str: String,
    encrypted: bool,
) -> Result<String, String> {
    let four_states = state.four_states()?;
    let file = open_access_str_file(&access_str).map_err(|e| format!("无法打开文件{}:{}", access_str, e))?;
    let size = file_size(&file)?;
    let (mimetype, file) = file_mimetype(file)?;
    let stream = file_to_stream(file);
    state.task_pool().spawn(async move {
        add_attachment(
            four_states,
            Arc::new(event),
            &id,
            encrypted,
            size,
            mimetype,
            stream,
        )
        .await;
    })
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
) -> Result<String, String> {
    let four_states = state.four_states()?;
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
    state.task_pool().spawn(async move {
        add_attachment(
            four_states,
            Arc::new(event),
            &id,
            encrypted,
            len as u64,
            mimetype,
            stream,
        )
        .await;
    })
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
) -> Result<(), String> {
    let client = state.get_client()?;
    delete_attachment(
        &state.diary_cache(),
        &state.local_file_cache(),
        &state.crypto(),
        &client,
        id,
        filename,
    )
    .await
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
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    encrypted: bool,
) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        use tauri_plugin_native_camera::NativeCameraExt;
        const MIMETYPE: &str = "image/jpeg";

        let result = app
            .native_camera()
            .take_picture()
            .map_err(|e| e.to_string())?;
        let base64_data = result.image_data;
        let binary_data = STANDARD.decode(base64_data).map_err(|e| e.to_string())?;
        let len = binary_data.len();
        let stream = create_mock_stream(binary_data, len);
        let three = state.four_states()?;
        state.task_pool().spawn(async move {
            add_attachment(
                three,
                Arc::new(event),
                &id,
                encrypted,
                len as u64,
                MIMETYPE.to_string(),
                stream,
            )
            .await;
        })
    }
    #[cfg(not(target_os = "android"))]
    {
        // 简单使用一下参数避免编译器警告
        let _ = (app, state, event, id, encrypted);
        Err("拍照功能仅在 Android 上可用".to_string())
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
) -> Result<String, String> {
    let four_states = state.four_states()?;
    state.task_pool().spawn(async move {
        toggle_attachment_encryption(four_states, Arc::new(event), &id, filename).await;
    })
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
) -> Result<String, String> {
    let four_states = state.four_states()?;
    state.task_pool().spawn(async move {
        rotate_image_attachment(four_states, Arc::new(event), &id, filename, rotation).await;
    })
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
) -> Result<String, String> {
    let lfc = state.local_file_cache();
    let client = state.get_client()?;
    state.task_pool().spawn(async move {
        caching_attachment(&lfc, &client, Arc::new(event), &id, &filename).await;
    })
}
