use super::client::MAX_MODELS_RESPONSE_BYTES;
use super::{
    AiAssistantMessage, AiCompletionDelta, AiCompletionRequest, AiError, AiMessage, AiModel,
    AiModelProvider, AiProviderConfig, AiToolCall, AiToolDefinition, AiToolResult, AiUsage,
    OpenAiCompatibleClient,
};
use serde_json::{json, Value};
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[tokio::test]
async fn lists_models_and_sends_optional_bearer_token() {
    let (base_url, request) = spawn_json_response(
        200,
        r#"{"data":[{"id":"qwen3:8b","owned_by":"ollama"},{"id":"local-model"}]}"#,
    )
    .await;
    let config = AiProviderConfig::new(&base_url, Some("test-secret".into())).unwrap();
    let client: Box<dyn AiModelProvider> = Box::new(OpenAiCompatibleClient::new(config).unwrap());

    let models = client.list_models().await.unwrap();

    assert_eq!(
        models,
        vec![
            AiModel {
                id: "qwen3:8b".into(),
                owned_by: Some("ollama".into()),
            },
            AiModel {
                id: "local-model".into(),
                owned_by: None,
            },
        ]
    );
    let request = request.await.unwrap();
    assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer test-secret\r\n"));
}

#[tokio::test]
async fn omits_authorization_header_when_api_key_is_missing() {
    let (base_url, request) = spawn_json_response(200, r#"{"data":[]}"#).await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();

    assert!(client.list_models().await.unwrap().is_empty());
    assert!(!request
        .await
        .unwrap()
        .to_ascii_lowercase()
        .contains("authorization:"));
}

#[tokio::test]
async fn treats_ollama_null_model_data_as_an_empty_list() {
    let (base_url, _) = spawn_json_response(200, r#"{"object":"list","data":null}"#).await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();

    assert!(client.list_models().await.unwrap().is_empty());
}

#[tokio::test]
async fn maps_openai_error_response_without_exposing_the_api_key() {
    let (base_url, _) = spawn_json_response(401, r#"{"error":{"message":"API Key 无效"}}"#).await;
    let config = AiProviderConfig::new(&base_url, Some("secret-not-in-error".into())).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();

    let error = client.list_models().await.unwrap_err();

    assert_eq!(
        error,
        AiError::HttpStatus {
            status: 401,
            message: "API Key 无效".into(),
        }
    );
    assert!(!error.to_string().contains("secret-not-in-error"));
}

#[tokio::test]
async fn rejects_invalid_model_list_response() {
    let (base_url, _) = spawn_json_response(200, r#"{"models":[]}"#).await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();

    assert!(matches!(
        client.list_models().await,
        Err(AiError::InvalidResponse(_))
    ));
}

#[tokio::test]
async fn rejects_empty_model_ids() {
    let (base_url, _) = spawn_json_response(200, r#"{"data":[{"id":"  "}]}"#).await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();

    assert_eq!(
        client.list_models().await,
        Err(AiError::InvalidResponse("模型列表中存在空的模型 ID".into()))
    );
}

#[tokio::test]
async fn rejects_oversized_model_list_before_reading_the_body() {
    let (base_url, _) =
        spawn_response(200, "{}", Some(MAX_MODELS_RESPONSE_BYTES.saturating_add(1))).await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();

    assert_eq!(
        client.list_models().await,
        Err(AiError::ResponseTooLarge {
            limit_bytes: MAX_MODELS_RESPONSE_BYTES,
        })
    );
}

#[tokio::test]
async fn completes_text_chat_and_maps_usage() {
    let (base_url, captured_request) = spawn_json_response(
        200,
        r#"{"choices":[{"message":{"role":"assistant","reasoning_content":"先检查日记列表","content":"找到 3 篇日记"},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":6,"total_tokens":26}}"#,
    )
    .await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client: Box<dyn AiModelProvider> = Box::new(OpenAiCompatibleClient::new(config).unwrap());
    let request = AiCompletionRequest::new(
        "qwen3:8b",
        vec![
            AiMessage::System("你是日记助手".into()),
            AiMessage::User("最近写了什么？".into()),
        ],
        vec![],
    )
    .unwrap();

    let completion = client.complete(request).await.unwrap();

    assert_eq!(
        completion.message.reasoning_content.as_deref(),
        Some("先检查日记列表")
    );
    assert_eq!(completion.message.content.as_deref(), Some("找到 3 篇日记"));
    assert!(completion.message.tool_calls.is_empty());
    assert_eq!(completion.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        completion.usage,
        Some(AiUsage {
            prompt_tokens: 20,
            completion_tokens: 6,
            total_tokens: 26,
        })
    );
    assert_eq!(
        captured_json_body(captured_request.await.unwrap()),
        json!({
            "model": "qwen3:8b",
            "messages": [
                {"role": "system", "content": "你是日记助手"},
                {"role": "user", "content": "最近写了什么？"}
            ],
            "reasoning_effort": "medium"
        })
    );
}

#[tokio::test]
async fn streams_text_deltas_and_maps_final_usage() {
    let body = concat!(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"reasoning_content\":\"先检查\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"日记列表\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"找到 \"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"3 篇日记\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":6,\"total_tokens\":26}}\n\n",
        "data: [DONE]\n\n",
    );
    let (base_url, captured_request) = spawn_sse_response(body).await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();
    let request = AiCompletionRequest::new(
        "qwen3:8b",
        vec![AiMessage::User("最近写了什么？".into())],
        vec![],
    )
    .unwrap();
    let received = Mutex::new(Vec::new());

    let completion = client
        .complete_stream(request, &|delta| {
            received.lock().unwrap().push(delta);
            Ok(())
        })
        .await
        .unwrap();

    assert_eq!(
        received.into_inner().unwrap(),
        vec![
            AiCompletionDelta::Reasoning("先检查".into()),
            AiCompletionDelta::Reasoning("日记列表".into()),
            AiCompletionDelta::Content("找到 ".into()),
            AiCompletionDelta::Content("3 篇日记".into()),
        ]
    );
    assert_eq!(
        completion.message.reasoning_content.as_deref(),
        Some("先检查日记列表")
    );
    assert_eq!(completion.message.content.as_deref(), Some("找到 3 篇日记"));
    assert_eq!(completion.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        completion.usage,
        Some(AiUsage {
            prompt_tokens: 20,
            completion_tokens: 6,
            total_tokens: 26,
        })
    );
    let body = captured_json_body(captured_request.await.unwrap());
    assert_eq!(body["reasoning_effort"], "medium");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
}

#[tokio::test]
async fn silently_falls_back_and_remembers_models_without_reasoning_support() {
    let completion_body =
        r#"{"choices":[{"message":{"content":"普通回答"},"finish_reason":"stop"}]}"#;
    let (base_url, captured_requests) = spawn_response_sequence(vec![
        MockResponse::json(
            400,
            r#"{"error":{"message":"unknown field reasoning_effort"}}"#,
        ),
        MockResponse::json(200, completion_body),
        MockResponse::json(200, completion_body),
    ])
    .await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();
    let request =
        AiCompletionRequest::new("plain-model", vec![AiMessage::User("测试".into())], vec![])
            .unwrap();

    client.complete(request.clone()).await.unwrap();
    client.complete(request).await.unwrap();

    let requests = captured_requests.await.unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        captured_json_body(requests[0].clone())["reasoning_effort"],
        "medium"
    );
    assert!(captured_json_body(requests[1].clone())
        .get("reasoning_effort")
        .is_none());
    assert!(captured_json_body(requests[2].clone())
        .get("reasoning_effort")
        .is_none());
}

#[tokio::test]
async fn silently_falls_back_for_streaming_models_without_reasoning_support() {
    let stream_body = concat!(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"普通回答\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let (base_url, captured_requests) = spawn_response_sequence(vec![
        MockResponse::json(422, r#"{"error":{"message":"reasoning is not supported"}}"#),
        MockResponse::sse(stream_body),
    ])
    .await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();
    let request =
        AiCompletionRequest::new("plain-model", vec![AiMessage::User("测试".into())], vec![])
            .unwrap();
    let received = Mutex::new(String::new());

    let completion = client
        .complete_stream(request, &|delta| {
            if let AiCompletionDelta::Content(content) = delta {
                received.lock().unwrap().push_str(&content);
            }
            Ok(())
        })
        .await
        .unwrap();

    assert_eq!(received.into_inner().unwrap(), "普通回答");
    assert_eq!(completion.message.content.as_deref(), Some("普通回答"));
    let requests = captured_requests.await.unwrap();
    assert_eq!(
        captured_json_body(requests[0].clone())["reasoning_effort"],
        "medium"
    );
    assert!(captured_json_body(requests[1].clone())
        .get("reasoning_effort")
        .is_none());
}

#[tokio::test]
async fn reconstructs_fragmented_streaming_tool_calls() {
    let body = concat!(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"name\":\"read_diary\",\"arguments\":\"{\\\"dia\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"ryId\\\":\\\"123\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let (base_url, _) = spawn_sse_response(body).await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();
    let request =
        AiCompletionRequest::new("model", vec![AiMessage::User("读取日记".into())], vec![])
            .unwrap();

    let completion = client.complete_stream(request, &|_| Ok(())).await.unwrap();

    assert_eq!(
        completion.message.tool_calls,
        vec![AiToolCall {
            id: "call-1".into(),
            name: "read_diary".into(),
            arguments: json!({"diaryId": "123"}),
        }]
    );
    assert_eq!(completion.finish_reason.as_deref(), Some("tool_calls"));
}

#[tokio::test]
async fn rejects_a_stream_that_ends_without_done_marker() {
    let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"半截回答\"}}]}\n\n";
    let (base_url, _) = spawn_sse_response(body).await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();
    let request =
        AiCompletionRequest::new("model", vec![AiMessage::User("测试".into())], vec![]).unwrap();

    assert_eq!(
        client.complete_stream(request, &|_| Ok(())).await,
        Err(AiError::InvalidResponse(
            "流式对话响应在 [DONE] 前意外结束".into()
        ))
    );
}

#[tokio::test]
async fn sends_tool_definitions_and_maps_string_encoded_tool_calls() {
    let (base_url, captured_request) = spawn_json_response(
        200,
        r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"search_diaries","arguments":"{\"query\":\"旅行\"}"}}]},"finish_reason":"tool_calls"}]}"#,
    )
    .await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();
    let request = AiCompletionRequest::new(
        "qwen3:8b",
        vec![AiMessage::User("查找旅行日记".into())],
        vec![AiToolDefinition {
            name: "search_diaries".into(),
            description: "根据关键词搜索日记".into(),
            parameters: json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"]
            }),
        }],
    )
    .unwrap();

    let completion = client.complete(request).await.unwrap();

    assert_eq!(
        completion.message.tool_calls,
        vec![AiToolCall {
            id: "call-1".into(),
            name: "search_diaries".into(),
            arguments: json!({"query": "旅行"}),
        }]
    );
    assert_eq!(completion.finish_reason.as_deref(), Some("tool_calls"));
    let body = captured_json_body(captured_request.await.unwrap());
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "search_diaries");
    assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
}

#[tokio::test]
async fn serializes_assistant_calls_and_tool_results_for_follow_up() {
    let (base_url, captured_request) = spawn_json_response(
        200,
        r#"{"choices":[{"message":{"role":"assistant","content":"没有找到相关日记"},"finish_reason":"stop"}]}"#,
    )
    .await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();
    let request = AiCompletionRequest::new(
        "model",
        vec![
            AiMessage::User("查找旅行日记".into()),
            AiMessage::Assistant(AiAssistantMessage {
                reasoning_content: Some("需要先搜索旅行日记".into()),
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

    client.complete(request).await.unwrap();

    let body = captured_json_body(captured_request.await.unwrap());
    assert_eq!(body["messages"][1]["role"], "assistant");
    assert_eq!(
        body["messages"][1]["reasoning_content"],
        "需要先搜索旅行日记"
    );
    assert_eq!(body["messages"][1]["tool_calls"][0]["id"], "call-1");
    assert_eq!(
        body["messages"][1]["tool_calls"][0]["function"]["arguments"],
        r#"{"query":"旅行"}"#
    );
    assert_eq!(body["messages"][2]["role"], "tool");
    assert_eq!(body["messages"][2]["tool_call_id"], "call-1");
}

#[tokio::test]
async fn rejects_empty_choices_and_invalid_tool_arguments() {
    for response in [
        r#"{"choices":[]}"#,
        r#"{"choices":[{"message":{"content":null,"tool_calls":[{"id":"call-1","function":{"name":"search","arguments":"not-json"}}]}}]}"#,
    ] {
        let (base_url, _) = spawn_json_response(200, response).await;
        let config = AiProviderConfig::new(&base_url, None).unwrap();
        let client = OpenAiCompatibleClient::new(config).unwrap();
        let request =
            AiCompletionRequest::new("model", vec![AiMessage::User("测试".into())], vec![])
                .unwrap();

        assert!(matches!(
            client.complete(request).await,
            Err(AiError::InvalidResponse(_))
        ));
    }
}

#[tokio::test]
async fn accepts_json_object_tool_arguments_from_compatible_services() {
    let (base_url, _) = spawn_json_response(
        200,
        r#"{"choices":[{"message":{"content":null,"tool_calls":[{"id":"call-1","function":{"name":"search","arguments":{"query":"测试"}}}]}}]}"#,
    )
    .await;
    let config = AiProviderConfig::new(&base_url, None).unwrap();
    let client = OpenAiCompatibleClient::new(config).unwrap();
    let request =
        AiCompletionRequest::new("model", vec![AiMessage::User("测试".into())], vec![]).unwrap();

    let completion = client.complete(request).await.unwrap();

    assert_eq!(
        completion.message.tool_calls[0].arguments,
        json!({"query": "测试"})
    );
}

struct MockResponse {
    status: u16,
    body: &'static str,
    content_type: &'static str,
}

impl MockResponse {
    fn json(status: u16, body: &'static str) -> Self {
        Self {
            status,
            body,
            content_type: "application/json",
        }
    }

    fn sse(body: &'static str) -> Self {
        Self {
            status: 200,
            body,
            content_type: "text/event-stream",
        }
    }
}

async fn spawn_response_sequence(
    responses: Vec<MockResponse>,
) -> (String, oneshot::Receiver<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (request_sender, request_receiver) = oneshot::channel();

    tokio::spawn(async move {
        let mut requests = Vec::with_capacity(responses.len());
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            requests.push(String::from_utf8(read_request(&mut socket).await).unwrap());
            let response_text =
                format_response(response.status, response.body, None, response.content_type);
            socket.write_all(response_text.as_bytes()).await.unwrap();
        }
        let _ = request_sender.send(requests);
    });

    (format!("http://{address}/v1"), request_receiver)
}

async fn spawn_json_response(
    status: u16,
    body: &'static str,
) -> (String, oneshot::Receiver<String>) {
    spawn_response(status, body, None).await
}

async fn spawn_sse_response(body: &'static str) -> (String, oneshot::Receiver<String>) {
    spawn_typed_response(200, body, None, "text/event-stream").await
}

async fn spawn_response(
    status: u16,
    body: &'static str,
    declared_content_length: Option<usize>,
) -> (String, oneshot::Receiver<String>) {
    spawn_typed_response(status, body, declared_content_length, "application/json").await
}

async fn spawn_typed_response(
    status: u16,
    body: &'static str,
    declared_content_length: Option<usize>,
    content_type: &'static str,
) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (request_sender, request_receiver) = oneshot::channel();

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_request(&mut socket).await;
        let _ = request_sender.send(String::from_utf8(request).unwrap());

        let response = format_response(status, body, declared_content_length, content_type);
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    (format!("http://{address}/v1"), request_receiver)
}

fn format_response(
    status: u16,
    body: &str,
    declared_content_length: Option<usize>,
    content_type: &str,
) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        422 => "Unprocessable Entity",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        declared_content_length.unwrap_or(body.len())
    )
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let (header_end, content_length) = loop {
        let read = socket.read(&mut buffer).await.unwrap();
        if read == 0 {
            return request;
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(header_end) = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| position + 4)
        {
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            break (header_end, content_length);
        }
    };

    while request.len() < header_end.saturating_add(content_length) {
        let read = socket.read(&mut buffer).await.unwrap();
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
    }
    request
}

fn captured_json_body(request: String) -> Value {
    let (_, body) = request.split_once("\r\n\r\n").unwrap();
    serde_json::from_str(body).unwrap()
}
