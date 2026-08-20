use super::session_agent::{AiSessionAgentOutcome, AiSessionAgentRunner};
use super::{
    AiAgent, AiAgentEvent, AiAgentRunResult, AiConversationTurn, AiError, AiModel, AiModelProvider,
    AiProviderConfig, DiaryReadTools, OpenAiCompatibleClient,
};
use crate::error::AppError;
use crate::state::AppState;
use tauri::ipc::Channel;
use tauri::State;
use tauri_plugin_log::log;

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
                Ok(AiAgentRunResult { response, source }) => {
                    let _ = send_event(&event, AiAgentEvent::ConversationSource(source));
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
/// * `base_url` - OpenAI 兼容 API 根地址
/// * `api_key` - 可选的 Bearer API Key
/// * `session_id` - 已创建的数字 AI 会话 ID；模型与历史消息从会话中读取
/// * `prompt` - 本轮用户问题
/// # Returns
/// * `Result<String, AppError>` - 后台问答任务令牌，可通过 `cmd_cancel_task` 取消
#[tauri::command]
#[specta::specta]
pub fn cmd_run_ai_session_agent(
    state: State<'_, AppState>,
    event: Channel<AiAgentEvent>,
    base_url: String,
    api_key: Option<String>,
    session_id: String,
    prompt: String,
) -> Result<String, AppError> {
    let config = AiProviderConfig::new(&base_url, api_key)?;
    let client = OpenAiCompatibleClient::new(config)?;
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

        match runner.run(&session_id, &prompt, cancellation, &emit).await {
            Ok(AiSessionAgentOutcome::Completed { response, source }) => {
                let _ = send_event(&event, AiAgentEvent::ConversationSource(source));
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
