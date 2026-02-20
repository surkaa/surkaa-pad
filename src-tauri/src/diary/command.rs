use crate::crypto::Crypto;
use crate::object::{NextToken, OssState};

use crate::diary::diary::{delete_diary, save_diary, update_diary_content_only};
use crate::diary::DiaryManifest;
use tauri::State;

/// 根据内容保存日记
/// # Arguments
/// * `content` - 日记内容
/// # Returns
/// * `Result<String, String>` - 成功时返回日记 UUID，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_save_diary(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    content: &str,
) -> Result<DiaryManifest, String> {
    let client = client.get_client()?;
    save_diary(
        &crypto,
        &client,
        content,
    ).await
}

/// 删除日记及其所有附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_delete_diary(
    client: State<'_, OssState>,
    uuid: String
) -> Result<(), String> {
    let client = client.get_client()?;
    delete_diary(
        &client,
        uuid
    ).await
}

/// 更新日记的内容
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `new_content` - 新的日记内容
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_update_diary_content_only(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
    new_content: &str,
) -> Result<DiaryManifest, String> {
    let client = client.get_client()?;
    update_diary_content_only(
        &crypto,
        &client,
        uuid,
        new_content
    ).await
}

/// 分页列出diary主键列表
/// # Arguments
/// * `next_token` - 分页的token
/// # Returns
/// * `Vec<String>` - diary主键列表
#[tauri::command]
#[specta::specta]
pub async fn cmd_list_diaries(
    client: tauri::State<'_, OssState>,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), String> {
    let client = client.get_client()?;
    let (objects, nt) = client
        .list("", next_token)
        .await
        .map_err(|e| format!("获取列表失败:{}", e))?;
    let keys = objects
        .into_iter()
        .map(|o| o.key().to_string())
        .collect();
    Ok((keys, nt))
}
