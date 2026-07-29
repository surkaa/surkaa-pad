use crate::error::AppError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AiError {
    #[error("AI 服务地址无效: {0}")]
    InvalidBaseUrl(String),
}

impl From<AiError> for AppError {
    fn from(error: AiError) -> Self {
        Self {
            error_type: "ai".into(),
            message: error.to_string(),
        }
    }
}
