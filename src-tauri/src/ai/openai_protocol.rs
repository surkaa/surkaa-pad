use super::{
    AiAssistantMessage, AiCompletion, AiCompletionRequest, AiError, AiMessage, AiModel, AiToolCall,
    AiToolDefinition, AiUsage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Deserialize)]
pub(super) struct ModelListResponse {
    data: Vec<ModelObject>,
}

impl ModelListResponse {
    pub(super) fn into_models(self) -> Result<Vec<AiModel>, AiError> {
        if self.data.iter().any(|model| model.id.trim().is_empty()) {
            return Err(AiError::InvalidResponse("模型列表中存在空的模型 ID".into()));
        }
        Ok(self
            .data
            .into_iter()
            .map(|model| AiModel {
                id: model.id,
                owned_by: model.owned_by,
            })
            .collect())
    }
}

#[derive(Deserialize)]
struct ModelObject {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
}

#[derive(Serialize)]
pub(super) struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatToolDefinition>,
}

impl From<&AiCompletionRequest> for ChatCompletionRequest {
    fn from(request: &AiCompletionRequest) -> Self {
        Self {
            model: request.model().to_owned(),
            messages: request.messages().iter().map(ChatMessage::from).collect(),
            tools: request
                .tools()
                .iter()
                .map(ChatToolDefinition::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<RequestToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

impl From<&AiMessage> for ChatMessage {
    fn from(message: &AiMessage) -> Self {
        match message {
            AiMessage::System(content) => Self::text("system", content),
            AiMessage::User(content) => Self::text("user", content),
            AiMessage::Assistant(message) => Self {
                role: "assistant",
                content: message.content.clone(),
                tool_calls: message
                    .tool_calls
                    .iter()
                    .map(RequestToolCall::from)
                    .collect(),
                tool_call_id: None,
            },
            AiMessage::Tool(result) => Self {
                role: "tool",
                content: Some(result.content.clone()),
                tool_calls: Vec::new(),
                tool_call_id: Some(result.tool_call_id.clone()),
            },
        }
    }
}

impl ChatMessage {
    fn text(role: &'static str, content: &str) -> Self {
        Self {
            role,
            content: Some(content.to_owned()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

#[derive(Serialize)]
struct RequestToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: RequestFunctionCall,
}

impl From<&AiToolCall> for RequestToolCall {
    fn from(call: &AiToolCall) -> Self {
        Self {
            id: call.id.clone(),
            kind: "function",
            function: RequestFunctionCall {
                name: call.name.clone(),
                arguments: call.arguments.to_string(),
            },
        }
    }
}

#[derive(Serialize)]
struct RequestFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct ChatToolDefinition {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatFunctionDefinition,
}

impl From<&AiToolDefinition> for ChatToolDefinition {
    fn from(tool: &AiToolDefinition) -> Self {
        Self {
            kind: "function",
            function: ChatFunctionDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
            },
        }
    }
}

#[derive(Serialize)]
struct ChatFunctionDefinition {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Deserialize)]
pub(super) struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

impl TryFrom<ChatCompletionResponse> for AiCompletion {
    type Error = AiError;

    fn try_from(response: ChatCompletionResponse) -> Result<Self, Self::Error> {
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AiError::InvalidResponse("对话响应不包含任何候选结果".into()))?;
        let message = choice.message.try_into()?;
        Ok(Self {
            message,
            finish_reason: choice.finish_reason,
            usage: response.usage.map(AiUsage::from),
        })
    }
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ResponseAssistantMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseAssistantMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ResponseToolCall>,
}

impl TryFrom<ResponseAssistantMessage> for AiAssistantMessage {
    type Error = AiError;

    fn try_from(message: ResponseAssistantMessage) -> Result<Self, Self::Error> {
        let mut tool_call_ids = HashSet::new();
        let mut tool_calls = Vec::with_capacity(message.tool_calls.len());
        for call in message.tool_calls {
            let call = AiToolCall::try_from(call)?;
            if !tool_call_ids.insert(call.id.clone()) {
                return Err(AiError::InvalidResponse(format!(
                    "对话响应包含重复的工具调用 ID: {}",
                    call.id
                )));
            }
            tool_calls.push(call);
        }

        let has_content = message
            .content
            .as_deref()
            .is_some_and(|content| !content.trim().is_empty());
        if !has_content && tool_calls.is_empty() {
            return Err(AiError::InvalidResponse(
                "助手响应不包含文本或工具调用".into(),
            ));
        }

        Ok(Self {
            content: message.content,
            tool_calls,
        })
    }
}

#[derive(Deserialize)]
struct ResponseToolCall {
    id: String,
    function: ResponseFunctionCall,
}

impl TryFrom<ResponseToolCall> for AiToolCall {
    type Error = AiError;

    fn try_from(call: ResponseToolCall) -> Result<Self, Self::Error> {
        if call.id.trim().is_empty() || call.function.name.trim().is_empty() {
            return Err(AiError::InvalidResponse("工具调用缺少 ID 或名称".into()));
        }
        let arguments = match call.function.arguments {
            FunctionArguments::Encoded(arguments) => serde_json::from_str(&arguments)
                .map_err(|_| AiError::InvalidResponse("工具调用参数不是有效的 JSON".into()))?,
            FunctionArguments::Json(arguments) => arguments,
        };
        if !arguments.is_object() {
            return Err(AiError::InvalidResponse(format!(
                "工具 {} 的调用参数必须是 JSON 对象",
                call.function.name
            )));
        }
        Ok(Self {
            id: call.id,
            name: call.function.name,
            arguments,
        })
    }
}

#[derive(Deserialize)]
struct ResponseFunctionCall {
    name: String,
    arguments: FunctionArguments,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FunctionArguments {
    Encoded(String),
    Json(Value),
}

#[derive(Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

impl From<ChatUsage> for AiUsage {
    fn from(usage: ChatUsage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        }
    }
}
