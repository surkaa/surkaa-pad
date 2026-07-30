use super::{
    AiAgent, AiAgentResponse, AiModel, AiModelProvider, AiProviderConfig, DiaryReadTools,
    OpenAiCompatibleClient,
};
use crate::error::AppError;
use crate::state::AppState;
use tauri::State;

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
/// * `prompt` - 用户问题
/// # Returns
/// * `Result<AiAgentResponse, AppError>` - 最终回答、模型调用轮数和 token 用量
#[tauri::command]
#[specta::specta]
pub async fn cmd_run_ai_agent(
    state: State<'_, AppState>,
    base_url: String,
    api_key: Option<String>,
    model: String,
    prompt: String,
) -> Result<AiAgentResponse, AppError> {
    let config = AiProviderConfig::new(&base_url, api_key)?;
    let client = OpenAiCompatibleClient::new(config)?;
    let tools = DiaryReadTools::new(state.inner().clone());
    Ok(AiAgent::new(&client, &tools).run(&model, &prompt).await?)
}
