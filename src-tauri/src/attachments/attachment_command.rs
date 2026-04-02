use crate::attachments::attachment::{add_attachment, caching_attachment, delete_attachment, rotate_image_attachment, save_decrypt_attachment, toggle_attachment_encryption, update_attachment_filename};
use crate::attachments::attachment_types::AttachmentProcessEvent;
use crate::diaries::get_diary;
use crate::state::AppState;
use crate::stream::{create_mock_stream, file_to_stream};
use crate::utils::{file_mimetype, file_size};
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
) -> Result<String, String> {
    let four_states = state.four_states()?;
    let fp = FilePath::from_str(&access_str).map_err(|e| format!("无效的文件路径: {}", e))?;
    let mut option = OpenOptions::new();
    option.read(true);
    let file = app_handle
        .fs()
        .open(fp, option)
        .map_err(|e| format!("无法打开文件: {}", e))?;
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
    app: AppHandle,
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
) -> Result<String, String> {
    let (crypto, cache, lfc, client) = state.four_states()?;
    let diary = get_diary(&cache, &lfc, &crypto, &client, &id).await?;
    let attachment = diary
        .attachments
        .iter()
        .find(|a| a.filename == filename)
        .ok_or_else(|| "附件不存在".to_string())?
        .clone();

    let ext = infer::get_from_mime(&attachment.mimetype)
        .map(|t| t.extension())
        .unwrap_or("");

    let filepath = app_handle
        .dialog()
        .file()
        .set_file_name(format!("{}.{}", attachment.filename, ext))
        .blocking_save_file()
        .ok_or("未选择".to_string())?;

    let mut option = OpenOptions::new();
    option.write(true).truncate(true).create(true);
    let file = app_handle
        .fs()
        .open(filepath, option)
        .map_err(|e| e.to_string())?;

    state.task_pool().spawn(async move {
        save_decrypt_attachment(
            (crypto, cache, lfc, client),
            Arc::new(event),
            &id,
            filename,
            attachment,
            file,
        )
        .await;
    })
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
) -> Result<(), String> {
    update_attachment_filename(
        state.four_states()?,
        &id,
        old_filename,
        new_filename,
        new_content
    ).await
}