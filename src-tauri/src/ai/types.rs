use super::AiError;
use serde::Serialize;
use serde_json::Value;
use specta::Type;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiModel {
    pub id: String,
    pub owned_by: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiCompletionRequest {
    model: String,
    messages: Vec<AiMessage>,
    tools: Vec<AiToolDefinition>,
}

impl AiCompletionRequest {
    pub fn new(
        model: impl Into<String>,
        messages: Vec<AiMessage>,
        tools: Vec<AiToolDefinition>,
    ) -> Result<Self, AiError> {
        let request = Self {
            model: model.into().trim().to_owned(),
            messages,
            tools,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn messages(&self) -> &[AiMessage] {
        &self.messages
    }

    pub fn tools(&self) -> &[AiToolDefinition] {
        &self.tools
    }

    fn validate(&self) -> Result<(), AiError> {
        if self.model.is_empty() {
            return Err(AiError::InvalidRequest("模型 ID 不能为空".into()));
        }
        if self.messages.is_empty() {
            return Err(AiError::InvalidRequest("对话消息不能为空".into()));
        }
        for message in &self.messages {
            validate_message(message)?;
        }

        let mut tool_names = HashSet::new();
        for tool in &self.tools {
            if tool.name.trim().is_empty() {
                return Err(AiError::InvalidRequest("工具名称不能为空".into()));
            }
            if tool.description.trim().is_empty() {
                return Err(AiError::InvalidRequest(format!(
                    "工具 {} 缺少说明",
                    tool.name
                )));
            }
            if !tool.parameters.is_object() {
                return Err(AiError::InvalidRequest(format!(
                    "工具 {} 的参数定义必须是 JSON 对象",
                    tool.name
                )));
            }
            if !tool_names.insert(tool.name.as_str()) {
                return Err(AiError::InvalidRequest(format!(
                    "工具名称重复: {}",
                    tool.name
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiMessage {
    System(String),
    User(String),
    Assistant(AiAssistantMessage),
    Tool(AiToolResult),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiAssistantMessage {
    pub content: Option<String>,
    pub tool_calls: Vec<AiToolCall>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiToolResult {
    pub tool_call_id: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiCompletion {
    pub message: AiAssistantMessage,
    pub finish_reason: Option<String>,
    pub usage: Option<AiUsage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiUsage {
    #[specta(type = f64)]
    pub prompt_tokens: u64,
    #[specta(type = f64)]
    pub completion_tokens: u64,
    #[specta(type = f64)]
    pub total_tokens: u64,
}

fn validate_message(message: &AiMessage) -> Result<(), AiError> {
    match message {
        AiMessage::System(content) | AiMessage::User(content) => {
            if content.trim().is_empty() {
                return Err(AiError::InvalidRequest("消息内容不能为空".into()));
            }
        }
        AiMessage::Assistant(message) => {
            let has_content = message
                .content
                .as_deref()
                .is_some_and(|content| !content.trim().is_empty());
            if !has_content && message.tool_calls.is_empty() {
                return Err(AiError::InvalidRequest(
                    "助手消息必须包含文本或工具调用".into(),
                ));
            }
            for call in &message.tool_calls {
                if call.id.trim().is_empty() || call.name.trim().is_empty() {
                    return Err(AiError::InvalidRequest("工具调用必须包含 ID 和名称".into()));
                }
                if !call.arguments.is_object() {
                    return Err(AiError::InvalidRequest(format!(
                        "工具 {} 的调用参数必须是 JSON 对象",
                        call.name
                    )));
                }
            }
        }
        AiMessage::Tool(result) => {
            if result.tool_call_id.trim().is_empty() {
                return Err(AiError::InvalidRequest(
                    "工具结果必须关联工具调用 ID".into(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_messages() -> Vec<AiMessage> {
        vec![AiMessage::User("查找最近的日记".into())]
    }

    #[test]
    fn completion_request_accepts_tools_and_trims_model_id() {
        let request = AiCompletionRequest::new(
            " qwen3:8b ",
            valid_messages(),
            vec![AiToolDefinition {
                name: "search_diaries".into(),
                description: "搜索日记".into(),
                parameters: json!({"type": "object", "properties": {}}),
            }],
        )
        .unwrap();

        assert_eq!(request.model(), "qwen3:8b");
        assert_eq!(request.tools().len(), 1);
    }

    #[test]
    fn completion_request_rejects_empty_model_and_messages() {
        assert_eq!(
            AiCompletionRequest::new(" ", valid_messages(), vec![]),
            Err(AiError::InvalidRequest("模型 ID 不能为空".into()))
        );
        assert_eq!(
            AiCompletionRequest::new("model", vec![], vec![]),
            Err(AiError::InvalidRequest("对话消息不能为空".into()))
        );
    }

    #[test]
    fn completion_request_rejects_invalid_or_duplicate_tools() {
        let duplicate_tools = vec![
            AiToolDefinition {
                name: "search".into(),
                description: "搜索".into(),
                parameters: json!({}),
            },
            AiToolDefinition {
                name: "search".into(),
                description: "再次搜索".into(),
                parameters: json!({}),
            },
        ];
        assert_eq!(
            AiCompletionRequest::new("model", valid_messages(), duplicate_tools),
            Err(AiError::InvalidRequest("工具名称重复: search".into()))
        );

        let invalid_schema = vec![AiToolDefinition {
            name: "search".into(),
            description: "搜索".into(),
            parameters: json!([]),
        }];
        assert!(matches!(
            AiCompletionRequest::new("model", valid_messages(), invalid_schema),
            Err(AiError::InvalidRequest(_))
        ));
    }

    #[test]
    fn completion_request_accepts_assistant_tool_calls_and_tool_results() {
        let request = AiCompletionRequest::new(
            "model",
            vec![
                AiMessage::Assistant(AiAssistantMessage {
                    content: None,
                    tool_calls: vec![AiToolCall {
                        id: "call-1".into(),
                        name: "search_diaries".into(),
                        arguments: json!({"query": "旅行"}),
                    }],
                }),
                AiMessage::Tool(AiToolResult {
                    tool_call_id: "call-1".into(),
                    content: r#"{"matches":[]}"#.into(),
                }),
            ],
            vec![],
        )
        .unwrap();

        assert_eq!(request.messages().len(), 2);
    }
}
