use super::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

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

struct ModelStartTimingProvider<'a> {
    model_started: &'a AtomicBool,
}

#[async_trait]
impl AiModelProvider for ModelStartTimingProvider<'_> {
    async fn list_models(&self) -> Result<Vec<AiModel>, AiError> {
        Ok(vec![])
    }

    async fn complete(&self, _request: AiCompletionRequest) -> Result<AiCompletion, AiError> {
        unreachable!("timing provider only supports streaming")
    }

    async fn complete_stream(
        &self,
        _request: AiCompletionRequest,
        on_delta: &(dyn Fn(AiCompletionDelta) -> Result<(), AiError> + Send + Sync),
    ) -> Result<AiCompletion, AiError> {
        assert!(!self.model_started.load(Ordering::Acquire));
        on_delta(AiCompletionDelta::Content("回答".into()))?;
        assert!(self.model_started.load(Ordering::Acquire));
        Ok(completion(Some("回答"), vec![], None))
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
    completion_with_reasoning(None, content, tool_calls, usage)
}

fn completion_with_reasoning(
    reasoning_content: Option<&str>,
    content: Option<&str>,
    tool_calls: Vec<AiToolCall>,
    usage: Option<AiUsage>,
) -> AiCompletion {
    AiCompletion {
        message: AiAssistantMessage {
            reasoning_content: reasoning_content.map(str::to_owned),
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
async fn keeps_the_connection_state_until_the_first_model_delta() {
    let model_started = AtomicBool::new(false);
    let provider = ModelStartTimingProvider {
        model_started: &model_started,
    };
    let tools = FakeTools::succeeding(json!({}));

    AiAgent::new(&provider, &tools)
        .run_stream("qwen", "测试连接状态", &|event| {
            if matches!(event, AiAgentEvent::ModelStarted { .. }) {
                model_started.store(true, Ordering::Release);
            }
            Ok(())
        })
        .await
        .unwrap();

    assert!(model_started.load(Ordering::Acquire));
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
async fn includes_completed_conversation_turns_before_the_current_prompt() {
    let provider = FakeProvider::new(vec![completion(Some("这是回答"), vec![], None)]);
    let tools = FakeTools::succeeding(json!({}));
    let history = vec![
        AiConversationTurn {
            user: " 第一问 ".into(),
            assistant: " 第一答 ".into(),
        },
        AiConversationTurn {
            user: "第二问".into(),
            assistant: "第二答".into(),
        },
    ];

    let result = AiAgent::new(&provider, &tools)
        .run_stream_with_history_source("qwen", &history, " 当前问题 ", &|_| Ok(()))
        .await
        .unwrap();

    let requests = provider.requests();
    assert_eq!(requests.len(), 1);
    assert!(matches!(&requests[0].messages()[0], AiMessage::System(_)));
    assert_eq!(
        &requests[0].messages()[1..],
        &[
            AiMessage::User("第一问".into()),
            AiMessage::Assistant(AiAssistantMessage {
                reasoning_content: None,
                content: Some("第一答".into()),
                tool_calls: vec![],
            }),
            AiMessage::User("第二问".into()),
            AiMessage::Assistant(AiAssistantMessage {
                reasoning_content: None,
                content: Some("第二答".into()),
                tool_calls: vec![],
            }),
            AiMessage::User("当前问题".into()),
        ]
    );
    assert_eq!(result.source.model, "qwen");
    assert_eq!(result.source.messages.len(), 7);
    assert!(matches!(
        result.source.messages.last(),
        Some(AiConversationSourceMessage::Assistant {
            content: Some(content),
            tool_calls,
            ..
        }) if content == "这是回答" && tool_calls.is_empty()
    ));
}

#[tokio::test]
async fn conversation_source_contains_tool_calls_raw_results_and_final_answer() {
    let provider = FakeProvider::new(vec![
        completion_with_reasoning(Some("需要读取日记"), None, vec![tool_call("call-1")], None),
        completion(Some("最终回答"), vec![], None),
    ]);
    let tools = FakeTools::succeeding(json!({"content": "完整正文"}));

    let result = AiAgent::new(&provider, &tools)
        .run_stream_with_history_source("qwen", &[], "读取日记", &|_| Ok(()))
        .await
        .unwrap();

    assert_eq!(result.source.tools.len(), 1);
    assert_eq!(result.source.tools[0].name, "read_diary");
    assert_eq!(result.source.tools[0].description, "读取日记");
    assert_eq!(
        serde_json::from_str::<Value>(&result.source.tools[0].parameters).unwrap(),
        json!({"type": "object"})
    );
    assert_eq!(result.source.messages.len(), 5);
    assert!(matches!(
        &result.source.messages[2],
        AiConversationSourceMessage::Assistant {
            reasoning_content: Some(reasoning),
            content: None,
            tool_calls,
        } if reasoning == "需要读取日记"
            && tool_calls.len() == 1
            && tool_calls[0].id == "call-1"
            && tool_calls[0].arguments == r#"{"diaryId":"123"}"#
    ));
    assert!(matches!(
        &result.source.messages[3],
        AiConversationSourceMessage::Tool {
            tool_call_id,
            content,
        } if tool_call_id == "call-1"
            && content.contains("完整正文")
            && content.contains("\"ok\":true")
    ));
    assert!(matches!(
        result.source.messages.last(),
        Some(AiConversationSourceMessage::Assistant {
            content: Some(content),
            ..
        }) if content == "最终回答"
    ));
}

#[tokio::test]
async fn rejects_incomplete_conversation_history_before_calling_the_provider() {
    let provider = FakeProvider::new(vec![]);
    let tools = FakeTools::succeeding(json!({}));
    let history = vec![AiConversationTurn {
        user: "上一问".into(),
        assistant: "  ".into(),
    }];

    assert_eq!(
        AiAgent::new(&provider, &tools)
            .run_stream_with_history("qwen", &history, "当前问题", &|_| Ok(()))
            .await,
        Err(AiError::InvalidRequest("第 1 轮历史问答不完整".into()))
    );
    assert!(provider.requests().is_empty());
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
async fn streams_model_tool_and_answer_events_in_order() {
    let provider = FakeProvider::new(vec![
        completion_with_reasoning(
            Some("需要先读取日记"),
            None,
            vec![tool_call("call-1")],
            None,
        ),
        completion_with_reasoning(Some("根据日记整理回答"), Some("最终回答"), vec![], None),
    ]);
    let tools = FakeTools::succeeding(json!({"content": "正文"}));
    let events = Mutex::new(Vec::new());

    let response = AiAgent::new(&provider, &tools)
        .run_stream("qwen", "读取日记", &|event| {
            events.lock().unwrap().push(event);
            Ok(())
        })
        .await
        .unwrap();

    assert_eq!(response.answer, "最终回答");
    let events = events.into_inner().unwrap();
    assert_eq!(events.len(), 9);
    assert_eq!(events[0], AiAgentEvent::ModelStarted { round: 1 });
    assert_eq!(
        events[1],
        AiAgentEvent::ReasoningDelta {
            round: 1,
            delta: "需要先读取日记".into(),
        }
    );
    assert!(matches!(
        &events[2],
        AiAgentEvent::ModelCompleted {
            round: 1,
            tool_count: 1,
            ..
        }
    ));
    assert_eq!(
        events[3],
        AiAgentEvent::ToolStarted {
            operation_id: 1,
            round: 1,
            title: "执行日记操作".into(),
            detail: None,
        }
    );
    assert!(matches!(
        &events[4],
        AiAgentEvent::ToolCompleted {
            operation_id: 1,
            summary,
            succeeded: true,
            ..
        } if summary == "操作完成"
    ));
    assert_eq!(events[5], AiAgentEvent::ModelStarted { round: 2 });
    assert_eq!(
        events[6],
        AiAgentEvent::ReasoningDelta {
            round: 2,
            delta: "根据日记整理回答".into(),
        }
    );
    assert_eq!(events[7], AiAgentEvent::AnswerDelta("最终回答".into()));
    assert!(matches!(
        &events[8],
        AiAgentEvent::ModelCompleted {
            round: 2,
            tool_count: 0,
            ..
        }
    ));
    let requests = provider.requests();
    assert!(matches!(
        &requests[1].messages()[2],
        AiMessage::Assistant(message)
            if message.reasoning_content.as_deref() == Some("需要先读取日记")
    ));
}

#[tokio::test]
async fn hides_internal_tool_error_details_from_the_model() {
    let provider = FakeProvider::new(vec![
        completion(None, vec![tool_call("call-1")], None),
        completion(Some("读取失败，请稍后重试"), vec![], None),
    ]);
    let tools = FakeTools::failing("C:\\Users\\name\\private diary.enc not found");
    let events = Mutex::new(Vec::new());

    AiAgent::new(&provider, &tools)
        .run_stream("qwen", "读取日记", &|event| {
            events.lock().unwrap().push(event);
            Ok(())
        })
        .await
        .unwrap();

    let requests = provider.requests();
    let AiMessage::Tool(result) = &requests[1].messages()[3] else {
        panic!("expected tool result")
    };
    assert!(result.content.contains("execution_failed"));
    assert!(!result.content.contains("C:\\Users"));
    assert!(!result.content.contains("private diary.enc"));
    let events = events.into_inner().unwrap();
    let failed_event = events
        .iter()
        .find_map(|event| match event {
            AiAgentEvent::ToolCompleted {
                summary,
                succeeded: false,
                ..
            } => Some(summary),
            _ => None,
        })
        .expect("failed tool event");
    assert_eq!(
        failed_event,
        "操作失败：C:\\Users\\name\\private diary.enc not found"
    );
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
