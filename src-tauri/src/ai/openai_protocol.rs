use super::{
    AiAssistantMessage, AiCompletion, AiCompletionRequest, AiError, AiMessage, AiModel, AiToolCall,
    AiToolDefinition, AiUsage,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

#[derive(Deserialize)]
pub(super) struct ModelListResponse {
    #[serde(deserialize_with = "deserialize_nullable_vec")]
    data: Vec<ModelObject>,
}

fn deserialize_nullable_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
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
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum ReasoningEffort {
    Medium,
}

impl ChatCompletionRequest {
    pub(super) fn new(request: &AiCompletionRequest, enable_reasoning: bool) -> Self {
        Self {
            model: request.model().to_owned(),
            messages: request.messages().iter().map(ChatMessage::from).collect(),
            tools: request
                .tools()
                .iter()
                .map(ChatToolDefinition::from)
                .collect(),
            reasoning_effort: enable_reasoning.then_some(ReasoningEffort::Medium),
            stream: false,
            stream_options: None,
        }
    }

    pub(super) fn streaming(request: &AiCompletionRequest, enable_reasoning: bool) -> Self {
        Self {
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
            ..Self::new(request, enable_reasoning)
        }
    }
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
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

#[derive(Deserialize)]
struct ChatCompletionChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct StreamChoice {
    #[serde(default)]
    index: usize,
    delta: StreamAssistantDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct StreamAssistantDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<StreamToolCallDelta>,
}

#[derive(Deserialize)]
struct StreamToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunctionDelta>,
}

#[derive(Deserialize)]
struct StreamFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<FunctionArgumentsDelta>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FunctionArgumentsDelta {
    Encoded(String),
    Json(Value),
}

#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub(super) struct ChatCompletionAccumulator {
    content: String,
    tool_calls: BTreeMap<usize, PartialToolCall>,
    finish_reason: Option<String>,
    usage: Option<AiUsage>,
    saw_primary_choice: bool,
}

impl ChatCompletionAccumulator {
    pub(super) fn push(&mut self, data: &str) -> Result<Vec<String>, AiError> {
        let chunk: ChatCompletionChunk = serde_json::from_str(data)
            .map_err(|error| AiError::InvalidResponse(error.to_string()))?;
        if let Some(usage) = chunk.usage {
            self.usage = Some(usage.into());
        }

        let mut content_deltas = Vec::new();
        for choice in chunk.choices {
            if choice.index != 0 {
                continue;
            }
            self.saw_primary_choice = true;
            if let Some(content) = choice.delta.content.filter(|content| !content.is_empty()) {
                self.content.push_str(&content);
                content_deltas.push(content);
            }
            for call in choice.delta.tool_calls {
                self.push_tool_call(call)?;
            }
            if choice.finish_reason.is_some() {
                self.finish_reason = choice.finish_reason;
            }
        }
        Ok(content_deltas)
    }

    fn push_tool_call(&mut self, delta: StreamToolCallDelta) -> Result<(), AiError> {
        let call = self.tool_calls.entry(delta.index).or_default();
        if let Some(id) = delta.id {
            call.id.push_str(&id);
        }
        if let Some(function) = delta.function {
            if let Some(name) = function.name {
                call.name.push_str(&name);
            }
            if let Some(arguments) = function.arguments {
                match arguments {
                    FunctionArgumentsDelta::Encoded(arguments) => {
                        call.arguments.push_str(&arguments)
                    }
                    FunctionArgumentsDelta::Json(arguments) => {
                        if !call.arguments.is_empty() {
                            return Err(AiError::InvalidResponse(
                                "流式工具调用混用了字符串和 JSON 参数".into(),
                            ));
                        }
                        call.arguments = arguments.to_string();
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<AiCompletion, AiError> {
        if !self.saw_primary_choice {
            return Err(AiError::InvalidResponse(
                "流式对话响应不包含候选结果".into(),
            ));
        }

        let mut tool_call_ids = HashSet::new();
        let mut tool_calls = Vec::with_capacity(self.tool_calls.len());
        for (_, call) in self.tool_calls {
            if call.id.trim().is_empty() || call.name.trim().is_empty() {
                return Err(AiError::InvalidResponse(
                    "流式工具调用缺少 ID 或名称".into(),
                ));
            }
            if !tool_call_ids.insert(call.id.clone()) {
                return Err(AiError::InvalidResponse(format!(
                    "流式对话响应包含重复的工具调用 ID: {}",
                    call.id
                )));
            }
            let arguments = if call.arguments.trim().is_empty() {
                Value::Object(Default::default())
            } else {
                serde_json::from_str(&call.arguments).map_err(|_| {
                    AiError::InvalidResponse("流式工具调用参数不是有效的 JSON".into())
                })?
            };
            if !arguments.is_object() {
                return Err(AiError::InvalidResponse(format!(
                    "工具 {} 的流式调用参数必须是 JSON 对象",
                    call.name
                )));
            }
            tool_calls.push(AiToolCall {
                id: call.id,
                name: call.name,
                arguments,
            });
        }

        let content = (!self.content.trim().is_empty()).then_some(self.content);
        if content.is_none() && tool_calls.is_empty() {
            return Err(AiError::InvalidResponse(
                "流式助手响应不包含文本或工具调用".into(),
            ));
        }
        Ok(AiCompletion {
            message: AiAssistantMessage {
                content,
                tool_calls,
            },
            finish_reason: self.finish_reason,
            usage: self.usage,
        })
    }
}
