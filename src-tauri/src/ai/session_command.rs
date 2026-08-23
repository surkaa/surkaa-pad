use super::{
    session_agent::persisted_conversation_source, AiSessionDetail, AiSessionMeta, AiToolExecutor,
    DiaryReadTools,
};
use crate::error::AppError;
use crate::state::AppState;
use chrono::Utc;
use tauri::State;

/// 创建一个空的 AI 会话。
/// # Arguments
/// * `title` - 会话的初始标题，通常取第一条用户问题
/// * `model` - 创建会话时选择的模型 ID
/// # Returns
/// * `Result<AiSessionMeta, AppError>` - 已加密持久化的会话元数据
#[tauri::command]
#[specta::specta]
pub async fn cmd_create_ai_session(
    state: State<'_, AppState>,
    title: String,
    model: String,
) -> Result<AiSessionMeta, AppError> {
    create_ai_session(state.inner(), title, model).await
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

/// 更新一个 AI 会话后续问答使用的模型，不改写已经保存的历史消息。
/// # Arguments
/// * `session_id` - 数字 AI 会话 ID
/// * `model` - 后续问答使用的新模型 ID
/// # Returns
/// * `Result<AiSessionMeta, AppError>` - 更新后的会话元数据
#[tauri::command]
#[specta::specta]
pub async fn cmd_update_ai_session_model(
    state: State<'_, AppState>,
    session_id: &str,
    model: String,
) -> Result<AiSessionMeta, AppError> {
    update_ai_session_model(state.inner(), session_id, model).await
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

async fn create_ai_session(
    state: &AppState,
    title: String,
    model: String,
) -> Result<AiSessionMeta, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state
        .ai_session_repository()
        .create_session(title, model, Utc::now().timestamp_millis())
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
        .map(|(meta, messages)| {
            let tools = DiaryReadTools::new_with_locked_storage(state.clone()).definitions();
            let conversation_source = persisted_conversation_source(&meta.model, &messages, &tools)
                .map_err(|error| {
                    tauri_plugin_log::log::warn!(
                        "重建 AI 会话源码失败: session_id={}, error={}",
                        meta.id,
                        error
                    );
                    error
                })
                .ok();
            AiSessionDetail {
                meta,
                messages,
                conversation_source,
            }
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

async fn update_ai_session_model(
    state: &AppState,
    session_id: &str,
    model: String,
) -> Result<AiSessionMeta, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state
        .ai_session_repository()
        .update_model(session_id, model, Utc::now().timestamp_millis())
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
    use crate::ai::{AiAssistantRecordState, AiConversationSourceMessage, AiSessionMessagePayload};
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
        let created = create_ai_session(&state, "第一条问题".into(), "test-model".into())
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

        let updated = update_ai_session_ai_title(&state, &created.id, Some("AI 生成的标题".into()))
            .await
            .unwrap();
        assert_eq!(updated.ai_title.as_deref(), Some("AI 生成的标题"));
        assert!(updated.updated_at >= created.updated_at);

        let updated = update_ai_session_model(&state, &created.id, "new-model".into())
            .await
            .unwrap();
        assert_eq!(updated.model, "new-model");

        delete_ai_session(&state, &created.id).await.unwrap();
        assert!(get_ai_session(&state, &created.id).await.unwrap().is_none());
        // 删除命令保持幂等，便于失败后重试。
        delete_ai_session(&state, &created.id).await.unwrap();
    }

    #[tokio::test]
    async fn loading_a_session_reconstructs_its_complete_model_source() {
        let temp = tempfile::tempdir().unwrap();
        let state = test_state(temp.path().to_path_buf());
        let created = create_ai_session(&state, "第一问".into(), "test-model".into())
            .await
            .unwrap();
        state
            .ai_session_repository()
            .append_message(
                &created.id,
                created.created_at + 1,
                AiSessionMessagePayload::User {
                    content: "第一问".into(),
                    timezone_offset_minutes: Some(480),
                },
            )
            .await
            .unwrap();
        state
            .ai_session_repository()
            .append_message(
                &created.id,
                created.created_at + 2,
                AiSessionMessagePayload::Assistant {
                    state: AiAssistantRecordState::Completed,
                    content: "第一答".into(),
                    error: None,
                    model: "test-model".into(),
                    usage: None,
                    context_tokens: None,
                    process_steps: vec![],
                    trace: vec![AiConversationSourceMessage::Assistant {
                        reasoning_content: Some("思考内容".into()),
                        content: Some("第一答".into()),
                        tool_calls: vec![],
                    }],
                },
            )
            .await
            .unwrap();

        let detail = get_ai_session(&state, &created.id).await.unwrap().unwrap();
        let source = detail.conversation_source.unwrap();

        assert_eq!(source.model, "test-model");
        assert_eq!(source.tools.len(), 4);
        assert_eq!(source.messages.len(), 4);
        assert!(matches!(
            source.messages[0],
            AiConversationSourceMessage::System { .. }
        ));
        assert!(matches!(
            source.messages[1],
            AiConversationSourceMessage::System { .. }
        ));
        assert_eq!(
            source.messages[2],
            AiConversationSourceMessage::User {
                content: "第一问".into()
            }
        );
        assert!(matches!(
            &source.messages[3],
            AiConversationSourceMessage::Assistant {
                reasoning_content: Some(reasoning),
                content: Some(content),
                tool_calls,
            } if reasoning == "思考内容" && content == "第一答" && tool_calls.is_empty()
        ));
    }
}
