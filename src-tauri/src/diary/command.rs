use std::sync::Arc;
use tauri::ipc::Channel;
use crate::crypto::Crypto;
use crate::object::{NextToken, OssState};

use crate::diary::diary::{delete_diary, save_diary, update_diary_content_only};
use crate::diary::diary_list::{get_diary_content, get_diary_summary, page_diary_ids};
use crate::diary::types::{DiarySummary, SearchDiariesEvent};
use crate::diary::{DiaryManifest, DiaryMemoryCache};
use tauri::State;
use crate::diary::diary_search::search_diaries;
use crate::task::TaskPool;

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
    save_diary(&crypto, &client, content).await
}

/// 删除日记及其所有附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_delete_diary(client: State<'_, OssState>, uuid: String) -> Result<(), String> {
    let client = client.get_client()?;
    delete_diary(&client, uuid).await
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
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
    new_content: &str,
) -> Result<DiaryManifest, String> {
    let client = client.get_client()?;
    update_diary_content_only(&cache, &crypto, &client, uuid, new_content).await
}

/// 分页列出diary主键列表
/// # Arguments
/// * `next_token` - 分页的token
/// # Returns
/// * `Vec<String>` - diary主键列表
#[tauri::command]
#[specta::specta]
pub async fn cmd_page_diary_ids(
    client: State<'_, OssState>,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), String> {
    let client = client.get_client()?;
    page_diary_ids(&client, next_token).await
}

/// 获取日记Summary
/// # Arguments
/// * `uuid` - 日记 UUID
/// # Returns
/// * `Result<DiarySummary, String>` - 成功时返回日记 Summary，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_diary_summary(
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
) -> Result<DiarySummary, String> {
    let client = client.get_client()?;
    get_diary_summary(&cache, &crypto, &client, uuid).await
}

/// 获取日记内容
/// # Arguments
/// * `uuid` - 日记 UUID
/// # Returns
/// * `Result<String, String>` - 成功时返回日记内容，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_diary_content(
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
) -> Result<String, String> {
    let client = client.get_client()?;
    get_diary_content(&cache, &crypto, &client, uuid).await
}

/// 搜索日记
/// # Arguments
/// * `keyword` - 搜索关键词，可通过空格分隔多个关键词
/// # Returns
/// * `Result<String, String>` - 成功时返回搜索任务token，可用于取消搜索任务，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_search_diaries(
    cache: State<'_, DiaryMemoryCache>,
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    event: Channel<SearchDiariesEvent>,
    keyword: String,
) -> Result<String, String> {
    let cache = cache.inner().clone();
    let crypto = crypto.inner().clone();
    let client = client.get_client()?;
    let event = event.clone();
    tp.spawn(async move {
        search_diaries(
            &cache,
            &crypto,
            &client,
            Arc::new(event),
            keyword,
        ).await;
    })
}
