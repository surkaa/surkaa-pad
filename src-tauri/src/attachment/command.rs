use crate::attachment::attachment::{add_attachment, delete_attachment, download_attachment};
use crate::attachment::types::DownloadAttachmentEvent;
use crate::crypto::Crypto;
use crate::diary::DiaryManifest;
use crate::object::OssState;
use crate::task::TaskPool;
use std::ops::Deref;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

/// 给日记添加附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `access_str` - 文件访问路径。
/// * `mimetype` - 附件 MIME 类型
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_add_attachment(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
    access_str: String,
    mimetype: String,
) -> Result<DiaryManifest, String> {
    let client = client.get_client()?;
    add_attachment(crypto.deref(), &client, uuid, access_str, mimetype).await
}

/// 下载日记附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `filename` - 附件 ID
/// * `nonce` - 解密iv
/// # Returns
/// * `Result<Vec<u8>, String>` - 成功时返回附件字节数据，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_download_attachment(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    app_handle: AppHandle,
    on_event: Channel<DownloadAttachmentEvent>,
    uuid: String,
    filename: String,
    nonce: Vec<u8>,
) -> Result<String, String> {
    let crypto = crypto.inner().clone();
    let client = client.get_client()?;
    tp.spawn(async move {
        download_attachment(
            crypto,
            client,
            &app_handle,
            Arc::new(on_event),
            uuid,
            filename,
            nonce,
        )
        .await;
    })
}

/// 删除日记的附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `filename` - 附件 ID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_delete_attachment(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
    file_name: String,
) -> Result<DiaryManifest, String> {
    let client = client.get_client()?;
    delete_attachment(crypto.deref(), &client, uuid, file_name).await
}
