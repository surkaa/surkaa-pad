use super::{AiToolCall, AiToolDefinition};
use async_trait::async_trait;
use serde_json::{json, Value};
use thiserror::Error;

const MAX_TOOL_ERROR_DISPLAY_CHARS: usize = 300;

#[async_trait]
pub trait AiToolExecutor: Send + Sync {
    fn definitions(&self) -> Vec<AiToolDefinition>;

    fn describe_call(&self, _call: &AiToolCall) -> AiToolCallDisplay {
        AiToolCallDisplay::new("执行日记操作", None)
    }

    fn summarize_result(&self, _call: &AiToolCall, result: Result<&Value, &AiToolError>) -> String {
        match result {
            Ok(_) => "操作完成".into(),
            Err(error) => format!("操作失败：{}", error.display_reason()),
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
    pub(crate) fn display_reason(&self) -> String {
        let reason = match self {
            Self::UnknownTool(tool) => format!("未知工具：{tool}"),
            Self::InvalidArguments { message, .. } => format!("参数无效：{message}"),
            Self::ExecutionFailed { message, .. } => message.clone(),
        };
        let compact = reason.split_whitespace().collect::<Vec<_>>().join(" ");
        if compact.is_empty() {
            return "未提供具体原因".into();
        }
        let mut chars = compact.chars();
        let truncated = chars
            .by_ref()
            .take(MAX_TOOL_ERROR_DISPLAY_CHARS)
            .collect::<String>();
        if chars.next().is_some() {
            format!("{truncated}…")
        } else {
            truncated
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bounded_user_visible_tool_failure_reasons() {
        let invalid = AiToolError::InvalidArguments {
            tool: "search_diaries".into(),
            message: "query\n不能为空".into(),
        };
        assert_eq!(invalid.display_reason(), "参数无效：query 不能为空");

        let long = AiToolError::ExecutionFailed {
            tool: "read_diary".into(),
            message: "错".repeat(MAX_TOOL_ERROR_DISPLAY_CHARS + 20),
        };
        assert_eq!(
            long.display_reason().chars().count(),
            MAX_TOOL_ERROR_DISPLAY_CHARS + 1
        );
        assert!(long.display_reason().ends_with('…'));
    }
}
