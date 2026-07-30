use super::{AiToolCall, AiToolDefinition};
use async_trait::async_trait;
use serde_json::{json, Value};
use thiserror::Error;

#[async_trait]
pub trait AiToolExecutor: Send + Sync {
    fn definitions(&self) -> Vec<AiToolDefinition>;

    async fn execute(&self, call: &AiToolCall) -> Result<Value, AiToolError>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AiToolError {
    #[error("未知工具: {0}")]
    UnknownTool(String),

    #[error("工具 {tool} 的参数无效: {message}")]
    InvalidArguments { tool: String, message: String },

    #[error("工具 {tool} 执行失败: {message}")]
    ExecutionFailed { tool: String, message: String },
}

impl AiToolError {
    pub(crate) fn response_for_model(&self) -> Value {
        match self {
            Self::UnknownTool(tool) => json!({
                "ok": false,
                "error": {
                    "code": "unknown_tool",
                    "message": format!("未知工具: {tool}"),
                },
            }),
            Self::InvalidArguments { tool, message } => json!({
                "ok": false,
                "error": {
                    "code": "invalid_arguments",
                    "message": format!("工具 {tool} 的参数无效: {message}"),
                },
            }),
            Self::ExecutionFailed { tool, .. } => json!({
                "ok": false,
                "error": {
                    "code": "execution_failed",
                    "message": format!("工具 {tool} 执行失败，请稍后重试"),
                },
            }),
        }
    }
}
