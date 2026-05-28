use crate::error::AppError;
use crate::object::NextToken;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::ipc::Channel;

use crate::diaries::diary::{delete_diary, save_diary, update_diary_content_only};
use crate::diaries::diary_list::{get_diary_content, get_diary_summary, page_diary_ids};
use crate::diaries::diary_search::search_diaries;
use crate::diaries::diary_types::{DiarySummary, SearchDiariesEvent};
use crate::state::AppState;
use tauri::State;

/// 根据内容保存日记
/// # Arguments
/// * `content` - 日记内容
/// # Returns
/// * `Result<(DiarySummary, String), String>` - 成功时返回日记 Summary 和日记 ID，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_save_diary(
    state: State<'_, AppState>,
    content: &str,
) -> Result<(DiarySummary, String), AppError> {
    let store = state.diary_store();
    Ok(save_diary(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        content,
    )
    .await?)
}

/// 删除日记及其所有附件
/// # Arguments
/// * `id` - 日记ID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_delete_diary(state: State<'_, AppState>, id: &str) -> Result<(), AppError> {
    let store = state.diary_store();
    Ok(delete_diary(&state.diary_cache(), &*store, id).await?)
}

/// 更新日记的内容
/// # Arguments
/// * `id` - 日记ID
/// * `new_content` - 新的日记内容
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_update_diary_content_only(
    state: State<'_, AppState>,
    id: &str,
    new_content: &str,
) -> Result<DiarySummary, AppError> {
    let store = state.diary_store();
    Ok(update_diary_content_only(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        id,
        new_content,
    )
    .await?)
}

/// 分页列出diary主键列表
/// # Arguments
/// * `count` - 每页的数量
/// * `next_token` - 分页的token
/// # Returns
/// * `Vec<String>` - diary主键列表
#[tauri::command]
#[specta::specta]
pub async fn cmd_page_diary_ids(
    state: State<'_, AppState>,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), AppError> {
    let store = state.diary_store();
    Ok(page_diary_ids(&*store, next_token).await?)
}

/// 获取日记Summary
/// # Arguments
/// * `id` - 日记ID
/// # Returns
/// * `Result<DiarySummary, String>` - 成功时返回日记 Summary，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_diary_summary(
    state: State<'_, AppState>,
    id: &str,
) -> Result<DiarySummary, AppError> {
    let store = state.diary_store();
    Ok(get_diary_summary(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        id,
    )
    .await?)
}

/// 获取日记内容
/// # Arguments
/// * `id` - 日记ID
/// # Returns
/// * `Result<(String, HashMap<String, String>), String>` - 成功时返回日记内容和附件filename->src Map，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_diary_content(
    state: State<'_, AppState>,
    id: &str,
) -> Result<(String, HashMap<String, String>), AppError> {
    let store = state.diary_store();
    Ok(get_diary_content(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        id,
    )
    .await?)
}

/// 搜索日记
/// # Arguments
/// * `keyword` - 搜索关键词，可通过空格分隔多个关键词
/// # Returns
/// * `Result<String, String>` - 成功时返回搜索任务token，可用于取消搜索任务，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_search_diaries(
    state: State<'_, AppState>,
    event: Channel<SearchDiariesEvent>,
    keyword: String,
    or: bool,
) -> Result<String, AppError> {
    let cache = state.diary_cache();
    let crypto = state.crypto();
    let store = state.diary_store();
    let event = event.clone();
    Ok(state.task_pool().spawn(async move {
        search_diaries(&cache, &crypto, &*store, Arc::new(event), keyword, or).await;
    })?)
}
