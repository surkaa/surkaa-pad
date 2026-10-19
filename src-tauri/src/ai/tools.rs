use super::{AiError, AiToolCall, AiToolDefinition};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use thiserror::Error;

const MAX_TOOL_ERROR_DISPLAY_CHARS: usize = 300;

#[async_trait]
pub trait AiToolExecutor: Send + Sync {
    fn definitions(&self) -> Vec<AiToolDefinition>;

    fn describe_call(&self, _call: &AiToolCall) -> AiToolCallDisplay {
        AiToolCallDisplay::new("执行工具", None)
    }

    fn summarize_result(&self, _call: &AiToolCall, result: Result<&Value, &AiToolError>) -> String {
        match result {
            Ok(_) => "操作完成".into(),
            Err(error) => format!("操作失败：{}", error.display_reason()),
        }
    }

    async fn execute(&self, call: &AiToolCall) -> Result<Value, AiToolError>;
}

/// 组合内置工具提供方；未注册的工具（包括关闭的联网工具）不会被执行。
pub struct AiToolSet {
    executors: Vec<Box<dyn AiToolExecutor>>,
    definitions: Vec<AiToolDefinition>,
    routes: HashMap<String, usize>,
}

impl AiToolSet {
    pub fn new(executors: Vec<Box<dyn AiToolExecutor>>) -> Result<Self, AiError> {
        let mut definitions = Vec::new();
        let mut routes = HashMap::new();
        for (index, executor) in executors.iter().enumerate() {
            for definition in executor.definitions() {
                if routes.insert(definition.name.clone(), index).is_some() {
                    return Err(AiError::InvalidRequest(format!(
                        "工具名称重复：{}",
                        definition.name
                    )));
                }
                definitions.push(definition);
            }
        }
        Ok(Self {
            executors,
            definitions,
            routes,
        })
    }

    fn executor(&self, call: &AiToolCall) -> Option<&dyn AiToolExecutor> {
        self.routes
            .get(&call.name)
            .map(|index| self.executors[*index].as_ref())
    }
}

#[async_trait]
impl AiToolExecutor for AiToolSet {
    fn definitions(&self) -> Vec<AiToolDefinition> {
        self.definitions.clone()
    }

    fn describe_call(&self, call: &AiToolCall) -> AiToolCallDisplay {
        self.executor(call)
            .map(|executor| executor.describe_call(call))
            .unwrap_or_else(|| AiToolCallDisplay::new("执行未知工具", None))
    }

    fn summarize_result(&self, call: &AiToolCall, result: Result<&Value, &AiToolError>) -> String {
        self.executor(call)
            .map(|executor| executor.summarize_result(call, result))
            .unwrap_or_else(|| "工具不可用".into())
    }

    async fn execute(&self, call: &AiToolCall) -> Result<Value, AiToolError> {
        self.executor(call)
            .ok_or_else(|| AiToolError::UnknownTool(call.name.clone()))?
            .execute(call)
            .await
    }
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

    /// 仅用于由应用生成的安全提示，不能包含上游响应、密钥或底层网络错误。
    #[error("工具 {tool} 不可用: {message}")]
    ServiceUnavailable { tool: String, message: String },
}

impl AiToolError {
    pub(crate) fn display_reason(&self) -> String {
        let reason = match self {
            Self::UnknownTool(tool) => format!("未知工具：{tool}"),
            Self::InvalidArguments { message, .. } => format!("参数无效：{message}"),
            Self::ExecutionFailed { message, .. } | Self::ServiceUnavailable { message, .. } => {
                message.clone()
            }
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
            Self::ServiceUnavailable { message, .. } => json!({
                "ok": false,
                "error": {
                    "code": "service_unavailable",
                    "message": message,
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

    struct TestTools(&'static str);
    #[async_trait]
    impl AiToolExecutor for TestTools {
        fn definitions(&self) -> Vec<AiToolDefinition> {
            vec![AiToolDefinition {
                name: self.0.into(),
                description: self.0.into(),
                parameters: json!({"type":"object"}),
            }]
        }
        fn describe_call(&self, _: &AiToolCall) -> AiToolCallDisplay {
            AiToolCallDisplay::new(self.0, None)
        }
        fn summarize_result(&self, _: &AiToolCall, _: Result<&Value, &AiToolError>) -> String {
            format!("{} completed", self.0)
        }
        async fn execute(&self, _: &AiToolCall) -> Result<Value, AiToolError> {
            Ok(json!({"provider":self.0}))
        }
    }

    #[tokio::test]
    async fn tool_set_preserves_registration_order_and_routes_execution_and_display() {
        let tools = AiToolSet::new(vec![
            Box::new(TestTools("diary")),
            Box::new(TestTools("web")),
        ])
        .unwrap();
        assert_eq!(
            tools
                .definitions()
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["diary", "web"]
        );
        for name in ["diary", "web"] {
            let call = AiToolCall {
                id: "1".into(),
                name: name.into(),
                arguments: json!({}),
            };
            let result = tools.execute(&call).await.unwrap();
            assert_eq!(result["provider"], name);
            assert_eq!(tools.describe_call(&call).title, name);
            assert_eq!(
                tools.summarize_result(&call, Ok(&result)),
                format!("{name} completed")
            );
        }
    }

    #[tokio::test]
    async fn absent_providers_cannot_be_called_and_duplicates_are_rejected() {
        let tools = AiToolSet::new(vec![Box::new(TestTools("diary"))]).unwrap();
        let call = AiToolCall {
            id: "1".into(),
            name: "search_web".into(),
            arguments: json!({"query":"test"}),
        };
        assert!(matches!(
            tools.execute(&call).await,
            Err(AiToolError::UnknownTool(_))
        ));
        assert!(AiToolSet::new(vec![
            Box::new(TestTools("duplicate")),
            Box::new(TestTools("duplicate"))
        ])
        .is_err());
    }

    #[test]
    fn only_safe_service_failures_are_explained_to_the_model() {
        let safe = AiToolError::ServiceUnavailable {
            tool: "search_web".into(),
            message: "余额不足，不要重复尝试".into(),
        };
        assert_eq!(
            safe.response_for_model()["error"]["message"],
            "余额不足，不要重复尝试"
        );
        let internal = AiToolError::ExecutionFailed {
            tool: "read_diary".into(),
            message: "private-secret".into(),
        };
        assert!(!internal
            .response_for_model()
            .to_string()
            .contains("private-secret"));
    }
}
