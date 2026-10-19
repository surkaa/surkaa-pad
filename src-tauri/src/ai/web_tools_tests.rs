use super::*;
use std::io::{Read, Write};

struct CapturedRequest {
    head: String,
    body: String,
}

fn spawn_server(
    response_body: &'static str,
) -> (String, std::sync::mpsc::Receiver<CapturedRequest>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let read = stream.read(&mut chunk).unwrap();
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let head = String::from_utf8_lossy(&buffer[..header_end + 4]).to_string();
            let content_length = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while buffer.len() < header_end + 4 + content_length {
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..read]);
            }
            let body = String::from_utf8_lossy(&buffer[header_end + 4..]).to_string();
            sender.send(CapturedRequest { head, body }).ok();
            break;
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream.write_all(response.as_bytes()).ok();
        stream.flush().ok();
    });
    (format!("http://127.0.0.1:{port}"), receiver)
}

fn web_tools(base_url: String) -> WebTools {
    WebTools::from_connection(
        &AiWebToolsConnection {
            base_url,
            api_key: Some("test-key".into()),
            max_results: 5,
        },
        None,
    )
    .unwrap()
}

fn tool_call(name: &str, arguments: Value) -> AiToolCall {
    AiToolCall {
        id: "call-1".into(),
        name: name.into(),
        arguments,
    }
}

fn search_call(arguments: Value) -> AiToolCall {
    tool_call(SEARCH_TOOL, arguments)
}

fn config(base_url: &str) -> WebToolConfig {
    WebToolConfig::new(base_url, "test-key", 5).unwrap()
}

#[test]
fn normalizes_base_urls_with_or_without_v1_suffix() {
    for base in [
        "https://api.deepseek.com",
        "https://api.deepseek.com/",
        "https://api.deepseek.com/v1",
        "https://api.deepseek.com/v1/",
    ] {
        assert_eq!(
            config(base).tools_url("search").as_str(),
            "https://api.deepseek.com/v1/tools/search"
        );
        assert_eq!(
            config(base).tools_url("fetch").as_str(),
            "https://api.deepseek.com/v1/tools/fetch"
        );
    }

    assert_eq!(
        config("https://proxy.example.com/deepseek/v1")
            .tools_url("search")
            .as_str(),
        "https://proxy.example.com/deepseek/v1/tools/search"
    );
}

#[test]
fn rejects_invalid_web_tool_configuration() {
    for url in [
        "ftp://api.deepseek.com",
        "https://user:secret@api.deepseek.com",
        "https://api.deepseek.com?token=secret",
        "https://api.deepseek.com#fragment",
    ] {
        assert!(matches!(
            WebToolConfig::new(url, "test-key", 5),
            Err(AiError::InvalidBaseUrl(_))
        ));
    }

    assert!(WebToolConfig::new("https://api.deepseek.com", "  ", 5).is_err());
    assert!(WebToolConfig::new("https://api.deepseek.com", "test-key", 0).is_err());
    assert!(WebToolConfig::new("https://api.deepseek.com", "test-key", 51).is_err());
}

#[test]
fn falls_back_to_chat_api_key_when_web_key_is_empty() {
    let connection = AiWebToolsConnection {
        base_url: "https://api.deepseek.com".into(),
        api_key: Some("  ".into()),
        max_results: 5,
    };
    assert!(WebToolConfig::from_connection(&connection, Some("chat-key")).is_ok());
    assert!(WebToolConfig::from_connection(&connection, None).is_err());

    let connection = AiWebToolsConnection {
        api_key: Some("web-key".into()),
        ..connection
    };
    assert!(WebToolConfig::from_connection(&connection, None).is_ok());
}

#[test]
fn search_body_validates_query_and_caps_count() {
    let call = search_call(json!({"query": "DeepSeek V4", "count": 10, "need_content": true}));
    let args: SearchArgs = parse_arguments(&call).unwrap();
    let (query, body) = build_search_body(&call, &args, 3).unwrap();

    assert_eq!(query, "DeepSeek V4");
    assert_eq!(body["query"], "DeepSeek V4");
    assert_eq!(body["count"], 3);
    assert_eq!(body["need_content"], true);
    assert!(body.get("ori_question").is_none());

    let call = search_call(json!({"query": "   "}));
    let args: SearchArgs = parse_arguments(&call).unwrap();
    assert!(build_search_body(&call, &args, 5).is_err());

    let call = search_call(json!({"query": "ok", "count": 51}));
    let args: SearchArgs = parse_arguments(&call).unwrap();
    assert!(build_search_body(&call, &args, 5).is_err());

    let long_query = "字".repeat(MAX_QUERY_CHARS + 1);
    let call = search_call(json!({"query": long_query}));
    let args: SearchArgs = parse_arguments(&call).unwrap();
    assert!(build_search_body(&call, &args, 5).is_err());
}

#[test]
fn search_body_includes_optional_original_question() {
    let call = search_call(json!({"query": "天气", "ori_question": "明天北京天气"}));
    let args: SearchArgs = parse_arguments(&call).unwrap();
    let (_, body) = build_search_body(&call, &args, 5).unwrap();

    assert_eq!(body["ori_question"], "明天北京天气");
}

#[test]
fn fetch_url_rejects_non_http_and_credentials() {
    for url in [
        "ftp://example.com/page",
        "https://user:secret@example.com/page",
        "not-a-url",
        "",
    ] {
        let call = tool_call(FETCH_TOOL, json!({"url": url}));
        assert!(
            validate_fetch_url(&call, url).is_err(),
            "url should fail: {url}"
        );
    }

    let call = tool_call(FETCH_TOOL, json!({"url": "https://example.com/page"}));
    assert_eq!(
        validate_fetch_url(&call, " https://example.com/page ").unwrap(),
        "https://example.com/page"
    );
}

#[test]
fn shapes_search_results_and_budgets_content() {
    let value = json!({
        "results": [
            {
                "title": "结果一",
                "url": "https://example.com/1",
                "snippet": "摘要",
                "summary": "长摘要",
                "content": "x".repeat(MAX_SEARCH_CONTENT_CHARS + 5),
                "site_name": "Example",
                "date": "2026-06-01T00:00:00+00:00",
                "authority": 3,
                "rerank_score": 2.4
            },
            {
                "title": "结果二",
                "url": "https://example.com/2",
                "content": "y".repeat(MAX_SEARCH_CONTENT_CHARS)
            },
            {
                "title": "结果三",
                "url": "https://example.com/3",
                "content": "z".repeat(MAX_SEARCH_CONTENT_CHARS)
            },
            {
                "title": "结果四",
                "url": "https://example.com/4",
                "content": "w".repeat(MAX_SEARCH_CONTENT_CHARS)
            }
        ]
    });
    let shaped = shape_search_response(value, "测试").unwrap();
    let results = shaped["results"].as_array().unwrap();

    assert_eq!(shaped["query"], "测试");
    assert_eq!(results.len(), 4);
    assert_eq!(results[0]["title"], "结果一");
    assert_eq!(results[0]["siteName"], "Example");
    assert_eq!(results[0]["authority"], 3);
    assert_eq!(results[0]["rerankScore"], 2.4);
    assert_eq!(
        results[0]["content"].as_str().unwrap().chars().count(),
        MAX_SEARCH_CONTENT_CHARS
    );
    assert_eq!(results[0]["contentTruncated"], true);
    // 总正文预算为 24000，前三项刚好用尽，第四项不再返回正文。
    assert!(results[3].get("content").is_none());
}

#[test]
fn shapes_fetch_response_and_truncates_content() {
    let value = json!({
        "title": "Example article",
        "content": "a".repeat(MAX_FETCH_CONTENT_CHARS + 10),
    });
    let shaped = shape_fetch_response(value, "https://example.com").unwrap();

    assert_eq!(shaped["title"], "Example article");
    assert_eq!(
        shaped["content"].as_str().unwrap().chars().count(),
        MAX_FETCH_CONTENT_CHARS
    );
    assert_eq!(shaped["truncated"], true);
}

#[test]
fn maps_http_errors_to_friendly_messages() {
    let body = br#"{"error":{"message":"'count' must be between 1 and 50","type":"invalid_request_error","param":null,"code":"invalid_request_error"}}"#;

    assert_eq!(
        http_error_message(401, b""),
        "API Key 无效或已过期（HTTP 401）"
    );
    assert_eq!(
        http_error_message(429, b""),
        "请求过于频繁，请稍后重试（HTTP 429）"
    );
    assert_eq!(
        http_error_message(400, body),
        "请求参数无效：'count' must be between 1 and 50"
    );
    assert_eq!(
        http_error_message(503, b""),
        "联网服务暂时不可用，请稍后重试（HTTP 503）"
    );
}

#[test]
fn exposes_search_and_fetch_tool_definitions() {
    let tools = WebTools::from_connection(
        &AiWebToolsConnection {
            base_url: "https://api.deepseek.com".into(),
            api_key: Some("test-key".into()),
            max_results: 5,
        },
        None,
    )
    .unwrap();
    let definitions = tools.definitions();

    assert_eq!(
        definitions
            .iter()
            .map(|definition| definition.name.as_str())
            .collect::<Vec<_>>(),
        [SEARCH_TOOL, FETCH_TOOL]
    );
    assert_eq!(
        definitions[0].parameters["properties"]["query"]["maxLength"],
        MAX_QUERY_CHARS
    );
}

#[test]
fn describes_calls_and_summarizes_results() {
    let tools = WebTools::from_connection(
        &AiWebToolsConnection {
            base_url: "https://api.deepseek.com".into(),
            api_key: Some("test-key".into()),
            max_results: 5,
        },
        None,
    )
    .unwrap();

    let display = tools.describe_call(&search_call(json!({"query": "天气"})));
    assert_eq!(display.title, "搜索网页");
    assert_eq!(display.detail.as_deref(), Some("“天气”"));

    let display = tools.describe_call(&tool_call(
        FETCH_TOOL,
        json!({"url": "https://example.com"}),
    ));
    assert_eq!(display.title, "抓取网页");
    assert_eq!(display.detail.as_deref(), Some("https://example.com"));

    let value = json!({"query": "天气", "results": [{}, {}]});
    assert_eq!(
        tools.summarize_result(&search_call(json!({"query": "天气"})), Ok(&value)),
        "找到 2 条网页结果"
    );

    let value = json!({"url": "https://example.com", "title": "示例页面", "content": ""});
    assert_eq!(
        tools.summarize_result(
            &tool_call(FETCH_TOOL, json!({"url": "https://example.com"})),
            Ok(&value)
        ),
        "已抓取“示例页面”"
    );
}

#[tokio::test]
async fn sends_search_request_to_expected_endpoint() {
    let (base_url, receiver) =
        spawn_server(r#"{"results":[{"title":"A","url":"https://a.example","summary":"摘要"}]}"#);
    let tools = web_tools(base_url);

    let result = tools
        .execute(&search_call(json!({"query": "DeepSeek V4"})))
        .await
        .unwrap();
    let request = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let head = request.head.to_ascii_lowercase();

    assert!(request.head.starts_with("POST /v1/tools/search HTTP/1.1"));
    assert!(head.contains("authorization: bearer test-key"));
    let body: Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(body["query"], "DeepSeek V4");
    assert_eq!(body["count"], 5);
    assert_eq!(body["need_content"], false);
    assert_eq!(result["results"][0]["title"], "A");
}

#[tokio::test]
async fn sends_fetch_request_and_shapes_page() {
    let (base_url, receiver) = spawn_server(r#"{"title":"Example","content":"hello"}"#);
    let tools = web_tools(base_url);

    let result = tools
        .execute(&tool_call(
            FETCH_TOOL,
            json!({"url": "https://example.com/page"}),
        ))
        .await
        .unwrap();
    let request = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();

    assert!(request.head.starts_with("POST /v1/tools/fetch HTTP/1.1"));
    let body: Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(body["url"], "https://example.com/page");
    assert_eq!(result["title"], "Example");
    assert_eq!(result["content"], "hello");
}
