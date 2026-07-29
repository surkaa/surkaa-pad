use crate::error::AppError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AiError {
    #[error("AI 服务地址无效: {0}")]
    InvalidBaseUrl(String),

    #[error("远程 AI 服务必须使用 HTTPS，以免日记内容和 API Key 被明文传输")]
    InsecureRemoteEndpoint,
}

impl From<AiError> for AppError {
    fn from(error: AiError) -> Self {
        Self {
            error_type: "ai".into(),
            message: error.to_string(),
        }
    }
}
