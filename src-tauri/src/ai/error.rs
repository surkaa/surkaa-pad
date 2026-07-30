use crate::error::AppError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AiError {
    #[error("AI 服务地址无效: {0}")]
    InvalidBaseUrl(String),

    #[error("AI 请求无效: {0}")]
    InvalidRequest(String),

    #[error("请求 AI 服务失败: {0}")]
    RequestFailed(String),

    #[error("AI 服务返回 HTTP {status}: {message}")]
    HttpStatus { status: u16, message: String },

    #[error("AI 服务响应超过大小限制（最大 {limit_bytes} 字节）")]
    ResponseTooLarge { limit_bytes: usize },

    #[error("解析 AI 服务响应失败: {0}")]
    InvalidResponse(String),

    #[error("AI Agent 在 {max_rounds} 轮内未能完成回答")]
    AgentRoundLimitExceeded { max_rounds: usize },

    #[error("发送 AI Agent 进度失败: {0}")]
    EventSendFailed(String),
}

impl From<AiError> for AppError {
    fn from(error: AiError) -> Self {
        Self {
            error_type: "ai".into(),
            message: error.to_string(),
        }
    }
}
