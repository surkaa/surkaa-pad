use crate::error::AppError;
use crate::object::NextToken;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::ipc::Channel;

use crate::diaries::diary::{delete_diary, save_diary, update_diary_content_only};
use crate::diaries::diary_list::{get_diary_content, get_diary_summary, page_diary_ids};
use crate::diaries::diary_search::{search_diaries, SearchDiaryQuery};
use crate::diaries::diary_types::{AttachmentTypeFilter, DiarySummary, SearchDiariesEvent};
use crate::diaries::DiaryContent;
use crate::state::AppState;
use tauri::State;

/// 根据内容保存日记
/// # Arguments
/// * `content` - 日记内容
/// # Returns
/// * `Result<(DiarySummary, DiaryContent), AppError>` - 成功时返回日记摘要和已保存的结构化内容
#[tauri::command]
#[specta::specta]
pub async fn cmd_save_diary(
    state: State<'_, AppState>,
    content: DiaryContent,
) -> Result<(DiarySummary, DiaryContent), AppError> {
    let store = state.diary_store();
    Ok(save_diary(&state.diary_cache(), &state.crypto(), &*store, content).await?)
}

/// 删除日记及其所有附件
/// # Arguments
/// * `id` - 日记ID
/// # Returns
/// * `Result<(), AppError>` - 成功时已删除日记及其全部附件
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
/// * `Result<DiarySummary, AppError>` - 成功时返回更新后的日记摘要
#[tauri::command]
#[specta::specta]
pub async fn cmd_update_diary_content_only(
    state: State<'_, AppState>,
    id: &str,
    new_content: DiaryContent,
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

/// 分页列出日记 ID
/// # Arguments
/// * `next_token` - 上一页返回的分页令牌，首页传入 `None`
/// # Returns
/// * `Result<(Vec<String>, NextToken), AppError>` - 日记 ID 列表和下一页令牌
#[tauri::command]
#[specta::specta]
pub async fn cmd_page_diary_ids(
    state: State<'_, AppState>,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), AppError> {
    let store = state.diary_store();
    Ok(page_diary_ids(&*store, next_token).await?)
}

/// 获取日记摘要
/// # Arguments
/// * `id` - 日记ID
/// # Returns
/// * `Result<DiarySummary, AppError>` - 成功时返回日记摘要
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_diary_summary(
    state: State<'_, AppState>,
    id: &str,
) -> Result<DiarySummary, AppError> {
    let store = state.diary_store();
    Ok(get_diary_summary(&state.diary_cache(), &state.crypto(), &*store, id).await?)
}

/// 获取日记内容
/// # Arguments
/// * `id` - 日记ID
/// # Returns
/// * `Result<(DiaryContent, HashMap<String, String>), AppError>` - 结构化日记内容和附件 ID 到本地 HTTP URL 的映射
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_diary_content(
    state: State<'_, AppState>,
    id: &str,
) -> Result<(DiaryContent, HashMap<String, String>), AppError> {
    let store = state.diary_store();
    Ok(get_diary_content(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        &state.attachment_server(),
        id,
    )
    .await?)
}

/// 搜索日记
/// # Arguments
/// * `event` - 接收搜索结果与错误事件的通道
/// * `keyword` - 搜索关键词，可通过空格分隔多个关键词
/// * `or` - `true` 表示匹配任意关键词，`false` 表示匹配全部关键词
/// * `attachment_types` - 需要匹配的附件类型，空列表表示不按附件类型过滤
/// * `attachment_or` - `true` 表示匹配任一附件类型，`false` 表示匹配全部附件类型
/// # Returns
/// * `Result<String, AppError>` - 搜索任务令牌，可用于取消搜索任务
#[tauri::command]
#[specta::specta]
pub fn cmd_search_diaries(
    state: State<'_, AppState>,
    event: Channel<SearchDiariesEvent>,
    keyword: String,
    or: bool,
    attachment_types: Vec<AttachmentTypeFilter>,
    attachment_or: bool,
) -> Result<String, AppError> {
    let cache = state.diary_cache();
    let crypto = state.crypto();
    let store = state.diary_store();
    let event = event.clone();
    Ok(state.task_pool().spawn(async move {
        search_diaries(
            &cache,
            &crypto,
            &*store,
            Arc::new(event),
            SearchDiaryQuery {
                keyword,
                keyword_or: or,
                attachment_types,
                attachment_or,
            },
        )
        .await;
    }))
}
