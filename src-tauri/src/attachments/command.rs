use crate::attachments::attachment::{
    add_attachment, delete_attachment, rotate_image_attachment, toggle_attachment_encryption,
};
use crate::attachments::types::AttachmentProcessEvent;
use crate::crypto::Crypto;
use crate::diaries::DiaryMemoryCache;
use crate::object::{OssState};
use crate::tasks::TaskPool;
use crate::utils::{open_file_stream, create_mock_stream};
use std::ops::Deref;
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
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    access_str: String,
    encrypted: bool,
) -> Result<String, String> {
    let cache = cache.inner().clone();
    let crypto = crypto.inner().clone();
    let client = client.get_client()?;
    let (file, mimetype, stream) = open_file_stream(access_str)?;
    tp.spawn(async move {
        add_attachment(
            cache,
            crypto,
            client,
            Arc::new(event),
            &id,
            encrypted,
            file,
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
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    data: Vec<u8>,
    mimetype: String,
    encrypted: bool,
) -> Result<String, String> {
    let cache = cache.inner().clone();
    let crypto = crypto.inner().clone();
    let client = client.get_client()?;
    let len = data.len();
    let stream = create_mock_stream(data, len);
    tp.spawn(async move {
        add_attachment(
            cache,
            crypto,
            client,
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
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    id: &str,
    filename: String,
) -> Result<(), String> {
    let client = client.get_client()?;
    delete_attachment(&cache, crypto.deref(), &client, id, filename).await
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
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
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
        let cache = cache.inner().clone();
        let crypto = crypto.inner().clone();
        let client = client.get_client()?;
        tp.spawn(async move {
            add_attachment(
                cache,
                crypto,
                client,
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
        let _ = (app, cache, crypto, client, tp, event, id, encrypted);
        Err("拍照功能仅在 Android 上可用".to_string())
    }
}

/// 将加密的附件转成未加密的、将未加密的附件转成加密的
/// # Arguments
/// * `id` - 日记 ID
/// * `filename` - 附件 ID
/// * `encrypted` - 是否需要加密
/// # Returns
/// * `Result<String, String>` - 成功时返回取消Token，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_toggle_attachment_encryption(
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    filename: String,
    encrypted: bool,
) -> Result<String, String> {
    let cache = cache.inner().clone();
    let crypto = crypto.inner().clone();
    let client = client.get_client()?;
    tp.spawn(async move {
        toggle_attachment_encryption(
            cache,
            crypto,
            client,
            Arc::new(event),
            &id,
            filename,
            encrypted,
        )
        .await;
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
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    filename: String,
    rotation: i32,
) -> Result<String, String> {
    let cache = cache.inner().clone();
    let crypto = crypto.inner().clone();
    let client = client.get_client()?;
    tp.spawn(async move {
        rotate_image_attachment(
            cache,
            crypto,
            client,
            Arc::new(event),
            &id,
            filename,
            rotation,
        )
        .await;
    })
}
