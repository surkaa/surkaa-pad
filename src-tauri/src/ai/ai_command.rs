use super::session_agent::{AiSessionAgentOutcome, AiSessionAgentRunner};
use super::{
    AiAgent, AiAgentEvent, AiAgentRunResult, AiContextWindow, AiConversationTurn, AiError, AiModel,
    AiModelProvider, AiProviderConfig, DiaryReadTools, OpenAiCompatibleClient,
};
use crate::error::AppError;
use crate::state::AppState;
use serde::Deserialize;
use specta::Type;
use tauri::ipc::Channel;
use tauri::State;
use tauri_plugin_log::log;

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionAgentConnection {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    #[specta(type = Option<f64>)]
    pub context_window_tokens: Option<u64>,
}

/// 获取 OpenAI 兼容服务提供的模型列表。
/// # Arguments
/// * `base_url` - OpenAI 兼容 API 根地址，例如 `http://localhost:11434/v1`
/// * `api_key` - 可选的 Bearer API Key；本地 Ollama 通常不需要
/// # Returns
/// * `Result<Vec<AiModel>, AppError>` - 服务返回的可用模型
#[tauri::command]
#[specta::specta]
pub async fn cmd_list_ai_models(
    base_url: String,
    api_key: Option<String>,
) -> Result<Vec<AiModel>, AppError> {
    let config = AiProviderConfig::new(&base_url, api_key)?;
    let client = OpenAiCompatibleClient::new(config)?;
    Ok(client.list_models().await?)
}

/// 尝试从 Ollama 原生 API 检测所选模型的上下文上限。
/// 非 Ollama 的 OpenAI 兼容服务没有统一的上下文长度接口，因此会正常返回 `None`。
#[tauri::command]
#[specta::specta]
pub async fn cmd_detect_ai_context_window(
    base_url: String,
    api_key: Option<String>,
    model: String,
) -> Result<Option<AiContextWindow>, AppError> {
    let config = AiProviderConfig::new(&base_url, api_key)?;
    let client = OpenAiCompatibleClient::new(config)?;
    Ok(client.detect_context_window(&model).await?)
}

/// 使用只读日记工具运行一次 AI Agent 问答。
/// # Arguments
/// * `base_url` - OpenAI 兼容 API 根地址
/// * `api_key` - 可选的 Bearer API Key
/// * `model` - 本次问答使用的模型 ID
/// * `history` - 当前会话中此前已完成的用户问题和最终回答
/// * `prompt` - 用户问题
/// * `event` - 接收模型状态、增量回答和最终结果的事件通道
/// # Returns
/// * `Result<String, AppError>` - 后台问答任务令牌，可通过 `cmd_cancel_task` 取消
#[tauri::command]
#[specta::specta]
pub fn cmd_run_ai_agent(
    state: State<'_, AppState>,
    event: Channel<AiAgentEvent>,
    base_url: String,
    api_key: Option<String>,
    model: String,
    history: Vec<AiConversationTurn>,
    prompt: String,
) -> Result<String, AppError> {
    let config = AiProviderConfig::new(&base_url, api_key)?;
    let client = OpenAiCompatibleClient::new(config)?;
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn_cancelable(move |cancellation| async move {
        let tools = DiaryReadTools::new(state);
        let agent = AiAgent::new(&client, &tools);
        let emit = |message| send_event(&event, message);
        let run = agent.run_stream_with_history_source(&model, &history, &prompt, &emit);

        tokio::select! {
            _ = cancellation.cancelled() => {
                let _ = send_event(&event, AiAgentEvent::Cancelled);
            }
            result = run => match result {
                Ok(AiAgentRunResult { response, .. }) => {
                    let _ = send_event(&event, AiAgentEvent::Completed(response));
                }
                Err(error) => {
                    log::warn!("AI Agent 运行失败: {error}");
                    let _ = send_event(&event, AiAgentEvent::Failed(error.to_string()));
                }
            }
        }
    }))
}

/// 在加密持久化会话中运行一次 AI Agent 问答。
/// Rust 任务会自动保存用户问题以及完成、失败或取消状态的助手消息；同一会话不能
/// 同时运行两个问答。任务运行期间存储模式保持不变。
/// # Arguments
/// * `event` - 接收模型状态、增量回答和最终结果的事件通道
/// * `connection` - 本轮的服务地址、可选 API Key、模型与可选上下文上限
/// * `session_id` - 已创建的数字 AI 会话 ID；历史消息从会话中读取
/// * `prompt` - 本轮用户问题
/// # Returns
/// * `Result<String, AppError>` - 后台问答任务令牌，可通过 `cmd_cancel_task` 取消
#[tauri::command]
#[specta::specta]
pub fn cmd_run_ai_session_agent(
    state: State<'_, AppState>,
    event: Channel<AiAgentEvent>,
    connection: AiSessionAgentConnection,
    session_id: String,
    prompt: String,
) -> Result<String, AppError> {
    let config = AiProviderConfig::new(&connection.base_url, connection.api_key)?;
    let client = OpenAiCompatibleClient::new(config)?;
    let context_window_tokens = normalize_context_window_tokens(connection.context_window_tokens)?;
    let model = connection.model;
    let repository = state.ai_session_repository();
    let run_guard = repository.try_begin_run(&session_id)?;
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    Ok(task_pool.spawn_cancelable(move |cancellation| async move {
        let _run_guard = run_guard;
        let storage_guard = tokio::select! {
            guard = state.lock_storage_operation() => guard,
            _ = cancellation.cancelled() => {
                let _ = send_event(&event, AiAgentEvent::Cancelled);
                return;
            }
        };
        let _storage_guard = storage_guard;
        let tools = DiaryReadTools::new_with_locked_storage(state);
        let runner = AiSessionAgentRunner::new(&repository, &client, &tools);
        let emit = |message| send_event(&event, message);

        match runner
            .run(
                &session_id,
                &model,
                context_window_tokens,
                &prompt,
                cancellation,
                &emit,
            )
            .await
        {
            Ok(AiSessionAgentOutcome::Completed { response }) => {
                let _ = send_event(&event, AiAgentEvent::Completed(response));
            }
            Ok(AiSessionAgentOutcome::Failed(message)) => {
                log::warn!("AI 会话问答失败并已持久化: {message}");
                let _ = send_event(&event, AiAgentEvent::Failed(message));
            }
            Ok(AiSessionAgentOutcome::Cancelled) => {
                let _ = send_event(&event, AiAgentEvent::Cancelled);
            }
            Err(error) => {
                log::warn!("AI 会话问答或持久化失败: {error}");
                let _ = send_event(&event, AiAgentEvent::Failed(error.to_string()));
            }
        }
    }))
}

fn send_event(event: &Channel<AiAgentEvent>, message: AiAgentEvent) -> Result<(), AiError> {
    event
        .send(message)
        .map_err(|error| AiError::EventSendFailed(error.to_string()))
}

fn normalize_context_window_tokens(value: Option<u64>) -> Result<Option<u64>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !(256..=10_000_000).contains(&value) {
        return Err(AiError::InvalidRequest(
            "AI 上下文上限必须是 256 到 10,000,000 之间的整数".into(),
        )
        .into());
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_optional_context_window_with_safe_bounds() {
        assert_eq!(normalize_context_window_tokens(None).unwrap(), None);
        assert_eq!(
            normalize_context_window_tokens(Some(256)).unwrap(),
            Some(256)
        );
        assert_eq!(
            normalize_context_window_tokens(Some(10_000_000)).unwrap(),
            Some(10_000_000)
        );

        for value in [0, 255, 10_000_001] {
            assert!(normalize_context_window_tokens(Some(value)).is_err());
        }
    }
}
