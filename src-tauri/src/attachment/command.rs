use crate::attachment::attachment::{add_attachment, delete_attachment};
use crate::attachment::types::AddAttachmentEvent;
use crate::crypto::Crypto;
use crate::diary::DiaryMemoryCache;
use crate::object::{create_mock_stream, OssState};
use crate::task::TaskPool;
use crate::utils::open_file_stream;
use std::ops::Deref;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::State;

/// 给日记添加附件
/// # Arguments
/// * `id` - 日记 ID
/// * `access_str` - 文件访问路径。
/// * `mimetype` - 附件 MIME 类型
/// * `encrypted` - 是否需要加密
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_add_attachment(
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    event: Channel<AddAttachmentEvent>,
    id: String,
    access_str: &str,
    mimetype: String,
    encrypted: bool,
) -> Result<String, String> {
    let cache = cache.inner().clone();
    let crypto = crypto.inner().clone();
    let client = client.get_client()?;
    let file = open_file_stream(access_str)?;
    tp.spawn(async move {
        add_attachment(
            cache,
            crypto,
            client,
            Arc::new(event),
            &id,
            &mimetype,
            encrypted,
            file,
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
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_add_attachment_memory(
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    event: Channel<AddAttachmentEvent>,
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
            &mimetype,
            encrypted,
            (len as u64, stream),
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
