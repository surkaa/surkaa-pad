use super::{AiToolCall, AiToolDefinition};
use async_trait::async_trait;
use serde_json::{json, Value};
use thiserror::Error;

#[async_trait]
pub trait AiToolExecutor: Send + Sync {
    fn definitions(&self) -> Vec<AiToolDefinition>;

    fn describe_call(&self, _call: &AiToolCall) -> AiToolCallDisplay {
        AiToolCallDisplay::new("执行日记操作", None)
    }

    fn summarize_result(&self, _call: &AiToolCall, result: Result<&Value, &AiToolError>) -> String {
        if result.is_ok() {
            "操作完成".into()
        } else {
            "操作失败，AI 将根据现有信息继续处理".into()
        }
    }

    async fn execute(&self, call: &AiToolCall) -> Result<Value, AiToolError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiToolCallDisplay {
    pub title: String,
    pub detail: Option<String>,
}

impl AiToolCallDisplay {
    pub fn new(title: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            title: title.into(),
            detail,
        }
    }
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
