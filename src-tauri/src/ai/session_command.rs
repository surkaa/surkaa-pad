use super::{AiSessionDetail, AiSessionMessagePage, AiSessionMeta};
use crate::error::AppError;
use crate::state::AppState;
use chrono::Utc;
use tauri::State;

/// 创建一个空的 AI 会话。
/// # Arguments
/// * `title` - 会话的初始标题，通常取第一条用户问题
/// # Returns
/// * `Result<AiSessionMeta, AppError>` - 已加密持久化的会话元数据
#[tauri::command]
#[specta::specta]
pub async fn cmd_create_ai_session(
    state: State<'_, AppState>,
    title: String,
) -> Result<AiSessionMeta, AppError> {
    create_ai_session(state.inner(), title).await
}

/// 按最近更新时间从新到旧列出 AI 会话。
/// # Returns
/// * `Result<Vec<AiSessionMeta>, AppError>` - 当前存储模式下可见的会话元数据
#[tauri::command]
#[specta::specta]
pub async fn cmd_list_ai_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<AiSessionMeta>, AppError> {
    list_ai_sessions(state.inner()).await
}

/// 读取一个 AI 会话及其全部消息。
/// # Arguments
/// * `session_id` - 数字 AI 会话 ID
/// # Returns
/// * `Result<Option<AiSessionDetail>, AppError>` - 会话不存在时返回 `None`
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_ai_session(
    state: State<'_, AppState>,
    session_id: &str,
) -> Result<Option<AiSessionDetail>, AppError> {
    get_ai_session(state.inner(), session_id).await
}

/// 读取会话 `meta.enc` 解密后的原始 JSON。
/// # Arguments
/// * `session_id` - 数字 AI 会话 ID
/// # Returns
/// * `Result<Option<AiSessionMeta>, AppError>` - 会话不存在时返回 `None`
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_ai_session_meta(
    state: State<'_, AppState>,
    session_id: &str,
) -> Result<Option<AiSessionMeta>, AppError> {
    get_ai_session_meta(state.inner(), session_id).await
}

/// 懒加载一页会话持久化消息。
/// # Arguments
/// * `session_id` - 数字 AI 会话 ID
/// * `offset` - 从零开始的消息索引（最大为 2^32 - 1）
/// * `limit` - 本页消息数，范围为 1–20
/// # Returns
/// * `Result<Option<AiSessionMessagePage>, AppError>` - 会话不存在时返回 `None`
#[tauri::command]
#[specta::specta]
pub async fn cmd_list_ai_session_messages(
    state: State<'_, AppState>,
    session_id: &str,
    offset: u32,
    limit: u32,
) -> Result<Option<AiSessionMessagePage>, AppError> {
    list_ai_session_messages(
        state.inner(),
        session_id,
        u64::from(offset),
        u64::from(limit),
    )
    .await
}

/// 更新 AI 为会话生成的标题。
/// # Arguments
/// * `session_id` - 数字 AI 会话 ID
/// * `ai_title` - AI 生成的标题；传入 `None` 可清除
/// # Returns
/// * `Result<AiSessionMeta, AppError>` - 更新后的会话元数据
#[tauri::command]
#[specta::specta]
pub async fn cmd_update_ai_session_ai_title(
    state: State<'_, AppState>,
    session_id: &str,
    ai_title: Option<String>,
) -> Result<AiSessionMeta, AppError> {
    update_ai_session_ai_title(state.inner(), session_id, ai_title).await
}

/// 删除一个 AI 会话的全部消息块及元数据。
/// # Arguments
/// * `session_id` - 数字 AI 会话 ID
/// # Returns
/// * `Result<(), AppError>` - 删除操作可安全重复执行
#[tauri::command]
#[specta::specta]
pub async fn cmd_delete_ai_session(
    state: State<'_, AppState>,
    session_id: &str,
) -> Result<(), AppError> {
    delete_ai_session(state.inner(), session_id).await
}

async fn create_ai_session(state: &AppState, title: String) -> Result<AiSessionMeta, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state
        .ai_session_repository()
        .create_session(title, Utc::now().timestamp_millis())
        .await?)
}

async fn list_ai_sessions(state: &AppState) -> Result<Vec<AiSessionMeta>, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state.ai_session_repository().list_sessions().await?)
}

async fn get_ai_session(
    state: &AppState,
    session_id: &str,
) -> Result<Option<AiSessionDetail>, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state
        .ai_session_repository()
        .load_session(session_id)
        .await?
        .map(|(meta, messages)| AiSessionDetail { meta, messages }))
}

async fn get_ai_session_meta(
    state: &AppState,
    session_id: &str,
) -> Result<Option<AiSessionMeta>, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state
        .ai_session_repository()
        .load_persisted_meta(session_id)
        .await?)
}

async fn list_ai_session_messages(
    state: &AppState,
    session_id: &str,
    offset: u64,
    limit: u64,
) -> Result<Option<AiSessionMessagePage>, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    let repository = state.ai_session_repository();
    if repository.load_persisted_meta(session_id).await?.is_none() {
        return Ok(None);
    }
    let (messages, total_count) = repository
        .load_message_page(session_id, offset, limit)
        .await?;
    Ok(Some(AiSessionMessagePage {
        messages,
        total_count,
    }))
}

async fn update_ai_session_ai_title(
    state: &AppState,
    session_id: &str,
    ai_title: Option<String>,
) -> Result<AiSessionMeta, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state
        .ai_session_repository()
        .update_ai_title(session_id, ai_title, Utc::now().timestamp_millis())
        .await?)
}

async fn delete_ai_session(state: &AppState, session_id: &str) -> Result<(), AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state
        .ai_session_repository()
        .delete_session(session_id)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::AiSessionMessagePayload;
    use crate::caches::LocalObjectStore;
    use crate::cryptos::Crypto;
    use crate::object::OssClient;

    fn test_state(path: std::path::PathBuf) -> AppState {
        let crypto = Crypto::new();
        crypto
            .derive_dek(
                "ai-session-command-password".into(),
                "YWktc2Vzc2lvbi1jb21tYW5kLXNhbHQ",
            )
            .unwrap();
        AppState::from_parts(crypto, OssClient::new(), LocalObjectStore::new(path))
    }

    #[tokio::test]
    async fn command_boundary_creates_lists_loads_updates_and_deletes_a_session() {
        let temp = tempfile::tempdir().unwrap();
        let state = test_state(temp.path().to_path_buf());
        let created = create_ai_session(&state, "第一条问题".into())
            .await
            .unwrap();
        state
            .ai_session_repository()
            .append_message(
                &created.id,
                created.created_at + 1,
                AiSessionMessagePayload::User {
                    content: "第一条问题".into(),
                    timezone_offset_minutes: None,
                },
            )
            .await
            .unwrap();

        let listed = list_ai_sessions(&state).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);

        let detail = get_ai_session(&state, &created.id).await.unwrap().unwrap();
        assert_eq!(detail.meta.committed_message_count, 1);
        assert_eq!(detail.messages.len(), 1);

        let persisted_meta = get_ai_session_meta(&state, &created.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted_meta, detail.meta);
        let message_page = list_ai_session_messages(&state, &created.id, 0, 5)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(message_page.total_count, 1);
        assert_eq!(message_page.messages, detail.messages);

        let updated = update_ai_session_ai_title(&state, &created.id, Some("AI 生成的标题".into()))
            .await
            .unwrap();
        assert_eq!(updated.ai_title.as_deref(), Some("AI 生成的标题"));
        assert!(updated.updated_at >= created.updated_at);

        delete_ai_session(&state, &created.id).await.unwrap();
        assert!(get_ai_session(&state, &created.id).await.unwrap().is_none());
        assert!(get_ai_session_meta(&state, &created.id)
            .await
            .unwrap()
            .is_none());
        assert!(list_ai_session_messages(&state, &created.id, 0, 5)
            .await
            .unwrap()
            .is_none());
        // 删除命令保持幂等，便于失败后重试。
        delete_ai_session(&state, &created.id).await.unwrap();
    }

    #[tokio::test]
    async fn details_messages_are_paginated_without_reading_the_full_session() {
        let temp = tempfile::tempdir().unwrap();
        let state = test_state(temp.path().to_path_buf());
        let created = create_ai_session(&state, "第一问".into()).await.unwrap();
        for index in 0..17 {
            state
                .ai_session_repository()
                .append_message(
                    &created.id,
                    created.created_at + index + 1,
                    AiSessionMessagePayload::User {
                        content: format!("消息 {index}"),
                        timezone_offset_minutes: None,
                    },
                )
                .await
                .unwrap();
        }

        for (offset, limit, expected_indexes) in [
            (0, 5, (0..5).collect::<Vec<_>>()),
            (5, 5, (5..10).collect::<Vec<_>>()),
            (10, 5, (10..15).collect::<Vec<_>>()),
            (15, 20, (15..17).collect::<Vec<_>>()),
        ] {
            let page = list_ai_session_messages(&state, &created.id, offset, limit)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(page.total_count, 17);
            assert_eq!(
                page.messages
                    .into_iter()
                    .map(|message| message.index)
                    .collect::<Vec<_>>(),
                expected_indexes,
            );
        }

        assert!(list_ai_session_messages(&state, &created.id, 0, 0)
            .await
            .is_err());
        assert!(list_ai_session_messages(&state, &created.id, 0, 21)
            .await
            .is_err());
    }
}
