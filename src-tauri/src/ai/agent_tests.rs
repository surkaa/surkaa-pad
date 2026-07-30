use super::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Mutex;

struct FakeProvider {
    responses: Mutex<VecDeque<Result<AiCompletion, AiError>>>,
    requests: Mutex<Vec<AiCompletionRequest>>,
}

impl FakeProvider {
    fn new(responses: Vec<AiCompletion>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().map(Ok).collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<AiCompletionRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl AiModelProvider for FakeProvider {
    async fn list_models(&self) -> Result<Vec<AiModel>, AiError> {
        Ok(vec![])
    }

    async fn complete(&self, request: AiCompletionRequest) -> Result<AiCompletion, AiError> {
        self.requests.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("fake provider response")
    }
}

struct FakeTools {
    result: Result<Value, AiToolError>,
    calls: Mutex<Vec<AiToolCall>>,
}

impl FakeTools {
    fn succeeding(result: Value) -> Self {
        Self {
            result: Ok(result),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn failing(message: &str) -> Self {
        Self {
            result: Err(AiToolError::ExecutionFailed {
                tool: "read_diary".into(),
                message: message.into(),
            }),
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl AiToolExecutor for FakeTools {
    fn definitions(&self) -> Vec<AiToolDefinition> {
        vec![AiToolDefinition {
            name: "read_diary".into(),
            description: "读取日记".into(),
            parameters: json!({"type": "object"}),
        }]
    }

    async fn execute(&self, call: &AiToolCall) -> Result<Value, AiToolError> {
        self.calls.lock().unwrap().push(call.clone());
        match &self.result {
            Ok(value) => Ok(value.clone()),
            Err(AiToolError::ExecutionFailed { tool, message }) => {
                Err(AiToolError::ExecutionFailed {
                    tool: tool.clone(),
                    message: message.clone(),
                })
            }
            Err(_) => unreachable!("test helper only configures execution failures"),
        }
    }
}

fn completion(
    content: Option<&str>,
    tool_calls: Vec<AiToolCall>,
    usage: Option<AiUsage>,
) -> AiCompletion {
    AiCompletion {
        message: AiAssistantMessage {
            content: content.map(str::to_owned),
            tool_calls,
        },
        finish_reason: None,
        usage,
    }
}

fn tool_call(id: &str) -> AiToolCall {
    AiToolCall {
        id: id.into(),
        name: "read_diary".into(),
        arguments: json!({"diaryId": "123"}),
    }
}

#[tokio::test]
async fn returns_direct_answer_without_running_tools() {
    let provider = FakeProvider::new(vec![completion(Some("这是回答"), vec![], None)]);
    let tools = FakeTools::succeeding(json!({}));

    let response = AiAgent::new(&provider, &tools)
        .run("qwen", "最近写了什么？")
        .await
        .unwrap();

    assert_eq!(response.answer, "这是回答");
    assert_eq!(response.model_rounds, 1);
    assert!(tools.calls.lock().unwrap().is_empty());
    let requests = provider.requests();
    assert_eq!(requests.len(), 1);
    assert!(matches!(&requests[0].messages()[0], AiMessage::System(_)));
}

#[tokio::test]
async fn executes_tools_and_feeds_results_back_to_the_model() {
    let provider = FakeProvider::new(vec![
        completion(
            None,
            vec![tool_call("call-1")],
            Some(AiUsage {
                prompt_tokens: 10,
                completion_tokens: 2,
                total_tokens: 12,
            }),
        ),
        completion(
            Some("日记内容是……"),
            vec![],
            Some(AiUsage {
                prompt_tokens: 20,
                completion_tokens: 4,
                total_tokens: 24,
            }),
        ),
    ]);
    let tools = FakeTools::succeeding(json!({"content": "正文"}));

    let response = AiAgent::new(&provider, &tools)
        .run("qwen", "读一下这篇日记")
        .await
        .unwrap();

    assert_eq!(response.answer, "日记内容是……");
    assert_eq!(response.model_rounds, 2);
    assert_eq!(
        response.usage,
        Some(AiUsage {
            prompt_tokens: 30,
            completion_tokens: 6,
            total_tokens: 36,
        })
    );
    let requests = provider.requests();
    assert_eq!(requests.len(), 2);
    assert!(matches!(
        &requests[1].messages()[2],
        AiMessage::Assistant(message) if message.tool_calls[0].id == "call-1"
    ));
    assert!(matches!(
        &requests[1].messages()[3],
        AiMessage::Tool(result)
            if result.tool_call_id == "call-1"
                && result.content.contains("\"ok\":true")
                && result.content.contains("正文")
    ));
}

#[tokio::test]
async fn hides_internal_tool_error_details_from_the_model() {
    let provider = FakeProvider::new(vec![
        completion(None, vec![tool_call("call-1")], None),
        completion(Some("读取失败，请稍后重试"), vec![], None),
    ]);
    let tools = FakeTools::failing("C:\\Users\\name\\private diary.enc not found");

    AiAgent::new(&provider, &tools)
        .run("qwen", "读取日记")
        .await
        .unwrap();

    let requests = provider.requests();
    let AiMessage::Tool(result) = &requests[1].messages()[3] else {
        panic!("expected tool result")
    };
    assert!(result.content.contains("execution_failed"));
    assert!(!result.content.contains("C:\\Users"));
    assert!(!result.content.contains("private diary.enc"));
}

#[tokio::test]
async fn stops_repeated_tool_calls_at_the_configured_round_limit() {
    let provider = FakeProvider::new(vec![
        completion(None, vec![tool_call("call-1")], None),
        completion(None, vec![tool_call("call-2")], None),
    ]);
    let tools = FakeTools::succeeding(json!({}));

    let error = AiAgent::new(&provider, &tools)
        .with_max_model_rounds(2)
        .run("qwen", "不停调用工具")
        .await
        .unwrap_err();

    assert_eq!(error, AiError::AgentRoundLimitExceeded { max_rounds: 2 });
    assert_eq!(tools.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn rejects_an_empty_user_prompt_before_calling_the_provider() {
    let provider = FakeProvider::new(vec![]);
    let tools = FakeTools::succeeding(json!({}));

    assert_eq!(
        AiAgent::new(&provider, &tools).run("qwen", " \n ").await,
        Err(AiError::InvalidRequest("问题不能为空".into()))
    );
    assert!(provider.requests().is_empty());
}
