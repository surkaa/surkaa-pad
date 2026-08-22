use super::AiError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiModel {
    pub id: String,
    pub owned_by: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiConversationTurn {
    pub user: String,
    pub assistant: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiConversationSource {
    pub model: String,
    pub messages: Vec<AiConversationSourceMessage>,
    pub tools: Vec<AiConversationSourceToolDefinition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    tag = "role"
)]
pub enum AiConversationSourceMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        reasoning_content: Option<String>,
        content: Option<String>,
        tool_calls: Vec<AiConversationSourceToolCall>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiConversationSourceToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiConversationSourceToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: String,
}

impl AiConversationSource {
    pub(crate) fn from_messages(
        model: &str,
        messages: &[AiMessage],
        tools: &[AiToolDefinition],
    ) -> Self {
        Self {
            model: model.to_owned(),
            messages: messages
                .iter()
                .map(AiConversationSourceMessage::from)
                .collect(),
            tools: tools
                .iter()
                .map(|tool| AiConversationSourceToolDefinition {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.parameters.to_string(),
                })
                .collect(),
        }
    }
}

impl From<&AiMessage> for AiConversationSourceMessage {
    fn from(message: &AiMessage) -> Self {
        match message {
            AiMessage::System(content) => Self::System {
                content: content.clone(),
            },
            AiMessage::User(content) => Self::User {
                content: content.clone(),
            },
            AiMessage::Assistant(message) => Self::Assistant {
                reasoning_content: message.reasoning_content.clone(),
                content: message.content.clone(),
                tool_calls: message
                    .tool_calls
                    .iter()
                    .map(|call| AiConversationSourceToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.to_string(),
                    })
                    .collect(),
            },
            AiMessage::Tool(result) => Self::Tool {
                tool_call_id: result.tool_call_id.clone(),
                content: result.content.clone(),
            },
        }
    }
}

impl TryFrom<&AiConversationSourceMessage> for AiMessage {
    type Error = AiError;

    fn try_from(message: &AiConversationSourceMessage) -> Result<Self, Self::Error> {
        let message = match message {
            AiConversationSourceMessage::System { content } => Self::System(content.clone()),
            AiConversationSourceMessage::User { content } => Self::User(content.clone()),
            AiConversationSourceMessage::Assistant {
                reasoning_content,
                content,
                tool_calls,
            } => Self::Assistant(AiAssistantMessage {
                reasoning_content: reasoning_content.clone(),
                content: content.clone(),
                tool_calls: tool_calls
                    .iter()
                    .map(|call| {
                        let arguments = serde_json::from_str(&call.arguments).map_err(|error| {
                            AiError::InvalidRequest(format!(
                                "历史工具 {} 的参数不是有效 JSON: {error}",
                                call.name
                            ))
                        })?;
                        Ok(AiToolCall {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            arguments,
                        })
                    })
                    .collect::<Result<Vec<_>, AiError>>()?,
            }),
            AiConversationSourceMessage::Tool {
                tool_call_id,
                content,
            } => Self::Tool(AiToolResult {
                tool_call_id: tool_call_id.clone(),
                content: content.clone(),
            }),
        };
        validate_message(&message)?;
        Ok(message)
    }
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
pub enum AiCompletionDelta {
    Reasoning(String),
    Content(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiAssistantMessage {
    pub reasoning_content: Option<String>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
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
                    reasoning_content: Some("需要先查找日记".into()),
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

    #[test]
    fn conversation_source_messages_roundtrip_tool_calls() {
        let original = AiMessage::Assistant(AiAssistantMessage {
            reasoning_content: Some("先读取".into()),
            content: None,
            tool_calls: vec![AiToolCall {
                id: "call-1".into(),
                name: "read_diary".into(),
                arguments: json!({"diaryId": "123"}),
            }],
        });
        let source = AiConversationSourceMessage::from(&original);

        assert_eq!(AiMessage::try_from(&source).unwrap(), original);
    }

    #[test]
    fn conversation_source_rejects_invalid_tool_arguments() {
        let source = AiConversationSourceMessage::Assistant {
            reasoning_content: None,
            content: None,
            tool_calls: vec![AiConversationSourceToolCall {
                id: "call-1".into(),
                name: "read_diary".into(),
                arguments: "not-json".into(),
            }],
        };

        assert!(matches!(
            AiMessage::try_from(&source),
            Err(AiError::InvalidRequest(message)) if message.contains("不是有效 JSON")
        ));
    }
}
