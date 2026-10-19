use super::{AiToolCall, AiToolDefinition};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
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

/// 将多个工具提供者组合成一个执行器。
///
/// 工具名称重复时保留先注册的定义，保证发给模型的工具列表合法。
pub struct CompositeToolExecutor {
    executors: Vec<Box<dyn AiToolExecutor>>,
    definitions: Vec<AiToolDefinition>,
    routes: HashMap<String, usize>,
}

impl CompositeToolExecutor {
    pub fn new(executors: Vec<Box<dyn AiToolExecutor>>) -> Self {
        let mut routes = HashMap::new();
        let mut definitions = Vec::new();
        for (index, executor) in executors.iter().enumerate() {
            for definition in executor.definitions() {
                if routes.contains_key(&definition.name) {
                    continue;
                }
                routes.insert(definition.name.clone(), index);
                definitions.push(definition);
            }
        }
        Self {
            executors,
            definitions,
            routes,
        }
    }

    fn executor_for(&self, name: &str) -> Option<&dyn AiToolExecutor> {
        self.routes
            .get(name)
            .map(|index| self.executors[*index].as_ref())
    }
}

#[async_trait]
impl AiToolExecutor for CompositeToolExecutor {
    fn definitions(&self) -> Vec<AiToolDefinition> {
        self.definitions.clone()
    }

    fn describe_call(&self, call: &AiToolCall) -> AiToolCallDisplay {
        self.executor_for(&call.name)
            .map(|executor| executor.describe_call(call))
            .unwrap_or_else(|| AiToolCallDisplay::new("执行工具操作", None))
    }

    fn summarize_result(&self, call: &AiToolCall, result: Result<&Value, &AiToolError>) -> String {
        self.executor_for(&call.name)
            .map(|executor| executor.summarize_result(call, result))
            .unwrap_or_else(|| "操作完成".into())
    }

    async fn execute(&self, call: &AiToolCall) -> Result<Value, AiToolError> {
        match self.executor_for(&call.name) {
            Some(executor) => executor.execute(call).await,
            None => Err(AiToolError::UnknownTool(call.name.clone())),
        }
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

    struct StubExecutor {
        name: &'static str,
    }

    #[async_trait]
    impl AiToolExecutor for StubExecutor {
        fn definitions(&self) -> Vec<AiToolDefinition> {
            vec![AiToolDefinition {
                name: self.name.into(),
                description: format!("{} 工具", self.name),
                parameters: json!({"type": "object"}),
            }]
        }

        fn describe_call(&self, _call: &AiToolCall) -> AiToolCallDisplay {
            AiToolCallDisplay::new(format!("调用 {}", self.name), None)
        }

        fn summarize_result(
            &self,
            _call: &AiToolCall,
            result: Result<&Value, &AiToolError>,
        ) -> String {
            if result.is_ok() {
                format!("{} 完成", self.name)
            } else {
                "失败".into()
            }
        }

        async fn execute(&self, call: &AiToolCall) -> Result<Value, AiToolError> {
            Ok(json!({"handledBy": self.name, "name": call.name}))
        }
    }

    fn stub_call(name: &str) -> AiToolCall {
        AiToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments: json!({}),
        }
    }

    #[tokio::test]
    async fn merges_definitions_and_routes_calls_by_name() {
        let tools = CompositeToolExecutor::new(vec![
            Box::new(StubExecutor { name: "first" }),
            Box::new(StubExecutor { name: "second" }),
        ]);

        assert_eq!(
            tools
                .definitions()
                .into_iter()
                .map(|definition| definition.name)
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(
            tools.describe_call(&stub_call("second")).title,
            "调用 second"
        );
        assert_eq!(
            tools.execute(&stub_call("second")).await.unwrap()["handledBy"],
            "second"
        );
        assert_eq!(
            tools.summarize_result(&stub_call("first"), Ok(&json!({}))),
            "first 完成"
        );
    }

    #[tokio::test]
    async fn keeps_first_definition_when_names_duplicate() {
        let tools = CompositeToolExecutor::new(vec![
            Box::new(StubExecutor { name: "same" }),
            Box::new(StubExecutor { name: "same" }),
        ]);

        assert_eq!(tools.definitions().len(), 1);
        assert_eq!(
            tools.execute(&stub_call("same")).await.unwrap()["handledBy"],
            "same"
        );
    }

    #[tokio::test]
    async fn returns_unknown_tool_for_unregistered_name() {
        let tools = CompositeToolExecutor::new(vec![Box::new(StubExecutor { name: "known" })]);

        assert_eq!(
            tools.execute(&stub_call("missing")).await,
            Err(AiToolError::UnknownTool("missing".into()))
        );
    }
}
