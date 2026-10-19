use super::{
    AiError, AiToolCall, AiToolCallDisplay, AiToolDefinition, AiToolError, AiToolExecutor,
};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use specta::Type;
use std::time::Duration;

const SEARCH_TOOL: &str = "web_search";
const FETCH_TOOL: &str = "web_fetch";
const DEFAULT_MAX_RESULTS: u64 = 5;
const MAX_SEARCH_RESULTS: u64 = 50;
const MAX_QUERY_CHARS: usize = 128;
const MAX_URL_CHARS: usize = 8192;
const MAX_SEARCH_CONTENT_CHARS: usize = 8_000;
const MAX_SEARCH_TOTAL_CONTENT_CHARS: usize = 24_000;
const MAX_FETCH_CONTENT_CHARS: usize = 24_000;
const MAX_RESPONSE_BYTES: usize = 2_000_000;
const MAX_ERROR_MESSAGE_CHARS: usize = 200;
const MAX_DISPLAY_CHARS: usize = 80;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// 前端传入的联网工具配置。API Key 留空时由 Agent 复用对话服务的 Key。
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiWebToolsConnection {
    pub base_url: String,
    pub api_key: Option<String>,
    #[serde(default = "default_max_results")]
    #[specta(type = f64)]
    pub max_results: u64,
}

#[derive(Clone, Debug)]
pub struct WebToolConfig {
    base_url: Url,
    api_key: String,
    max_results: usize,
}

impl WebToolConfig {
    pub fn new(base_url: &str, api_key: &str, max_results: u64) -> Result<Self, AiError> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err(AiError::InvalidRequest(
                "启用联网工具需要配置 DeepSeek API Key".into(),
            ));
        }
        if !(1..=MAX_SEARCH_RESULTS).contains(&max_results) {
            return Err(AiError::InvalidRequest(format!(
                "联网工具返回条数必须是 1 到 {MAX_SEARCH_RESULTS} 之间的整数"
            )));
        }

        let mut base_url = Url::parse(base_url.trim())
            .map_err(|error| AiError::InvalidBaseUrl(error.to_string()))?;
        if !matches!(base_url.scheme(), "http" | "https") {
            return Err(AiError::InvalidBaseUrl("仅支持 http 或 https 协议".into()));
        }
        if base_url.host_str().is_none() {
            return Err(AiError::InvalidBaseUrl("缺少主机名".into()));
        }
        if !base_url.username().is_empty() || base_url.password().is_some() {
            return Err(AiError::InvalidBaseUrl(
                "请通过 API Key 字段提供凭证，不要把凭证写入地址".into(),
            ));
        }
        if base_url.query().is_some() || base_url.fragment().is_some() {
            return Err(AiError::InvalidBaseUrl(
                "服务地址不能包含查询参数或片段".into(),
            ));
        }

        // 兼容用户直接填写 https://api.deepseek.com 或带 /v1 的地址，
        // 内部统一保存到 API 根路径，再拼接 /v1/tools/...。
        let path = base_url.path().trim_end_matches('/');
        let path = path.strip_suffix("/v1").unwrap_or(path);
        base_url.set_path(&if path.is_empty() {
            "/".to_owned()
        } else {
            format!("{path}/")
        });

        Ok(Self {
            base_url,
            api_key: api_key.to_owned(),
            max_results: max_results as usize,
        })
    }

    pub fn from_connection(
        connection: &AiWebToolsConnection,
        fallback_api_key: Option<&str>,
    ) -> Result<Self, AiError> {
        let api_key = connection
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .or_else(|| {
                fallback_api_key
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
            })
            .ok_or_else(|| {
                AiError::InvalidRequest("启用联网工具需要配置 DeepSeek API Key".into())
            })?;
        Self::new(&connection.base_url, api_key, connection.max_results)
    }

    fn tools_url(&self, name: &str) -> Url {
        self.base_url
            .join(&format!("v1/tools/{name}"))
            .expect("validated web tools base URL must accept relative paths")
    }
}

pub struct WebTools {
    client: reqwest::Client,
    config: WebToolConfig,
}

impl WebTools {
    pub fn from_connection(
        connection: &AiWebToolsConnection,
        fallback_api_key: Option<&str>,
    ) -> Result<Self, AiError> {
        let config = WebToolConfig::from_connection(connection, fallback_api_key)?;
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| AiError::RequestFailed(error.to_string()))?;
        Ok(Self { client, config })
    }

    async fn search(&self, call: &AiToolCall) -> Result<Value, AiToolError> {
        let args: SearchArgs = parse_arguments(call)?;
        let (query, body) = build_search_body(call, &args, self.config.max_results)?;
        let value = self.post_json(SEARCH_TOOL, "search", &body).await?;
        shape_search_response(value, &query)
    }

    async fn fetch(&self, call: &AiToolCall) -> Result<Value, AiToolError> {
        let args: FetchArgs = parse_arguments(call)?;
        let url = validate_fetch_url(call, &args.url)?;
        let value = self
            .post_json(FETCH_TOOL, "fetch", &json!({"url": url}))
            .await?;
        shape_fetch_response(value, &url)
    }

    async fn post_json(
        &self,
        tool: &str,
        endpoint: &str,
        body: &Value,
    ) -> Result<Value, AiToolError> {
        let response = self
            .client
            .post(self.config.tools_url(endpoint))
            .bearer_auth(&self.config.api_key)
            .json(body)
            .send()
            .await
            .map_err(|error| failed(tool, network_error_message(&error)))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| failed(tool, network_error_message(&error)))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(failed(tool, "联网服务响应过大，已停止解析"));
        }
        if !status.is_success() {
            return Err(failed(tool, http_error_message(status.as_u16(), &bytes)));
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| failed(tool, format!("解析联网服务响应失败：{error}")))
    }
}

#[async_trait]
impl AiToolExecutor for WebTools {
    fn definitions(&self) -> Vec<AiToolDefinition> {
        vec![
            AiToolDefinition {
                name: SEARCH_TOOL.into(),
                description: "使用 DeepSeek 搜索公开网页，返回标题、链接、摘要等结构化结果。需要最新信息、事实核查或查找资料时使用。".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "搜索词，支持 site:、lang:、after: 等搜索语法",
                            "minLength": 1,
                            "maxLength": MAX_QUERY_CHARS
                        },
                        "count": {
                            "type": "integer",
                            "description": "返回条数；不会超过用户在设置中配置的上限",
                            "minimum": 1,
                            "maximum": MAX_SEARCH_RESULTS
                        },
                        "need_content": {
                            "type": "boolean",
                            "description": "是否返回网页正文；正文较长时会截断",
                            "default": false
                        },
                        "ori_question": {
                            "type": "string",
                            "description": "触发搜索的用户原始问题，有助于理解搜索意图",
                            "maxLength": MAX_QUERY_CHARS
                        }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
            },
            AiToolDefinition {
                name: FETCH_TOOL.into(),
                description: "抓取指定 HTTP/HTTPS 网页并返回 Markdown 正文。需要阅读用户给出的链接或搜索结果中的具体页面时使用。".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "要抓取的 HTTP/HTTPS 网页地址",
                            "maxLength": MAX_URL_CHARS
                        }
                    },
                    "required": ["url"],
                    "additionalProperties": false
                }),
            },
        ]
    }

    fn describe_call(&self, call: &AiToolCall) -> AiToolCallDisplay {
        match call.name.as_str() {
            SEARCH_TOOL => AiToolCallDisplay::new(
                "搜索网页",
                call.arguments
                    .get("query")
                    .and_then(Value::as_str)
                    .map(|query| compact(query, MAX_DISPLAY_CHARS))
                    .filter(|query| !query.is_empty())
                    .map(|query| format!("“{query}”")),
            ),
            FETCH_TOOL => AiToolCallDisplay::new(
                "抓取网页",
                call.arguments
                    .get("url")
                    .and_then(Value::as_str)
                    .map(|url| compact(url, MAX_DISPLAY_CHARS))
                    .filter(|url| !url.is_empty()),
            ),
            _ => AiToolCallDisplay::new("执行联网操作", None),
        }
    }

    fn summarize_result(&self, call: &AiToolCall, result: Result<&Value, &AiToolError>) -> String {
        let value = match result {
            Ok(value) => value,
            Err(error) => return format!("操作失败：{}", error.display_reason()),
        };
        match call.name.as_str() {
            SEARCH_TOOL => value
                .get("results")
                .and_then(Value::as_array)
                .map(|results| format!("找到 {} 条网页结果", results.len()))
                .unwrap_or_else(|| "网页搜索完成".into()),
            FETCH_TOOL => value
                .get("title")
                .and_then(Value::as_str)
                .map(|title| compact(title, MAX_DISPLAY_CHARS))
                .filter(|title| !title.is_empty())
                .map(|title| format!("已抓取“{title}”"))
                .unwrap_or_else(|| "网页抓取完成".into()),
            _ => "操作完成".into(),
        }
    }

    async fn execute(&self, call: &AiToolCall) -> Result<Value, AiToolError> {
        match call.name.as_str() {
            SEARCH_TOOL => self.search(call).await,
            FETCH_TOOL => self.fetch(call).await,
            _ => Err(AiToolError::UnknownTool(call.name.clone())),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SearchArgs {
    query: String,
    count: Option<u64>,
    need_content: Option<bool>,
    ori_question: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FetchArgs {
    url: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchResult>,
}

#[derive(Debug, Deserialize)]
struct SearchResult {
    #[serde(default)]
    rerank_score: Option<f64>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    site_name: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    authority: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FetchResponse {
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
}

fn default_max_results() -> u64 {
    DEFAULT_MAX_RESULTS
}

fn parse_arguments<T>(call: &AiToolCall) -> Result<T, AiToolError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(call.arguments.clone())
        .map_err(|error| invalid_arguments(call, format!("参数格式无效：{error}")))
}

fn build_search_body(
    call: &AiToolCall,
    args: &SearchArgs,
    max_results: usize,
) -> Result<(String, Value), AiToolError> {
    let query = args.query.trim();
    if query.is_empty() {
        return Err(invalid_arguments(call, "query 不能为空"));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(invalid_arguments(
            call,
            format!("query 不能超过 {MAX_QUERY_CHARS} 个字符"),
        ));
    }

    let requested = args.count.unwrap_or(max_results as u64);
    if !(1..=MAX_SEARCH_RESULTS).contains(&requested) {
        return Err(invalid_arguments(
            call,
            format!("count 必须在 1 到 {MAX_SEARCH_RESULTS} 之间"),
        ));
    }
    let count = requested.min(max_results as u64);
    let mut body = json!({
        "query": query,
        "count": count,
        "need_content": args.need_content.unwrap_or(false),
    });
    if let Some(ori_question) = args
        .ori_question
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if ori_question.chars().count() > MAX_QUERY_CHARS {
            return Err(invalid_arguments(
                call,
                format!("ori_question 不能超过 {MAX_QUERY_CHARS} 个字符"),
            ));
        }
        body["ori_question"] = json!(ori_question);
    }

    Ok((query.to_owned(), body))
}

fn validate_fetch_url(call: &AiToolCall, raw: &str) -> Result<String, AiToolError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(invalid_arguments(call, "url 不能为空"));
    }
    if raw.chars().count() > MAX_URL_CHARS {
        return Err(invalid_arguments(
            call,
            format!("url 不能超过 {MAX_URL_CHARS} 个字符"),
        ));
    }
    let url =
        Url::parse(raw).map_err(|_| invalid_arguments(call, "url 不是有效的 HTTP/HTTPS 地址"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid_arguments(
            call,
            "url 必须是包含主机名的 HTTP/HTTPS 地址",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_arguments(call, "url 不能包含用户名或密码等凭证"));
    }
    Ok(url.to_string())
}

fn shape_search_response(value: Value, query: &str) -> Result<Value, AiToolError> {
    let response: SearchResponse = serde_json::from_value(value)
        .map_err(|error| failed(SEARCH_TOOL, format!("解析搜索结果失败：{error}")))?;
    let mut content_budget = MAX_SEARCH_TOTAL_CONTENT_CHARS;
    let mut results = Vec::with_capacity(response.results.len());

    for result in response.results {
        let mut item = Map::new();
        item.insert("title".into(), json!(result.title));
        item.insert("url".into(), json!(result.url));
        if !result.snippet.is_empty() {
            item.insert("snippet".into(), json!(result.snippet));
        }
        if !result.summary.is_empty() {
            item.insert("summary".into(), json!(result.summary));
        }
        if let Some(site_name) = result.site_name.filter(|name| !name.is_empty()) {
            item.insert("siteName".into(), json!(site_name));
        }
        if let Some(date) = result.date.filter(|date| !date.is_empty()) {
            item.insert("date".into(), json!(date));
        }
        if let Some(authority) = result.authority {
            item.insert("authority".into(), json!(authority));
        }
        if let Some(score) = result.rerank_score {
            item.insert("rerankScore".into(), json!(score));
        }
        if !result.content.is_empty() && content_budget > 0 {
            let limit = MAX_SEARCH_CONTENT_CHARS.min(content_budget);
            let (content, truncated) = truncate_chars(&result.content, limit);
            content_budget = content_budget.saturating_sub(content.chars().count());
            item.insert("content".into(), json!(content));
            if truncated {
                item.insert("contentTruncated".into(), json!(true));
            }
        }
        results.push(Value::Object(item));
    }

    Ok(json!({"query": query, "results": results}))
}

fn shape_fetch_response(value: Value, url: &str) -> Result<Value, AiToolError> {
    let response: FetchResponse = serde_json::from_value(value)
        .map_err(|error| failed(FETCH_TOOL, format!("解析网页内容失败：{error}")))?;
    let (content, truncated) = truncate_chars(&response.content, MAX_FETCH_CONTENT_CHARS);
    Ok(json!({
        "url": url,
        "title": response.title,
        "content": content,
        "truncated": truncated,
    }))
}

fn http_error_message(status: u16, body: &[u8]) -> String {
    let api_message = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|message| !message.trim().is_empty());

    let reason = match status {
        400 => "请求参数无效",
        401 => "API Key 无效或已过期",
        402 => "账户余额不足",
        403 => "请求被安全策略拒绝",
        404 => "未能获取可用的网页内容",
        408 => "请求超时",
        413 => "请求内容过大",
        429 => "请求过于频繁，请稍后重试",
        500..=599 => "联网服务暂时不可用，请稍后重试",
        _ => "联网请求失败",
    };

    match api_message {
        Some(message) => format!(
            "{reason}：{}",
            truncate_chars(message.trim(), MAX_ERROR_MESSAGE_CHARS).0
        ),
        None => format!("{reason}（HTTP {status}）"),
    }
}

fn network_error_message(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "联网请求超时".into()
    } else if error.is_connect() {
        "无法连接联网服务".into()
    } else {
        format!("联网请求失败：{error}")
    }
}

fn invalid_arguments(call: &AiToolCall, message: impl Into<String>) -> AiToolError {
    AiToolError::InvalidArguments {
        tool: call.name.clone(),
        message: message.into(),
    }
}

fn failed(tool: &str, message: impl Into<String>) -> AiToolError {
    AiToolError::ExecutionFailed {
        tool: tool.into(),
        message: message.into(),
    }
}

fn truncate_chars(text: &str, limit: usize) -> (String, bool) {
    let mut chars = text.chars();
    let truncated = chars.by_ref().take(limit).collect::<String>();
    let was_truncated = chars.next().is_some();
    (truncated, was_truncated)
}

fn compact(text: &str, limit: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[cfg(test)]
#[path = "web_tools_tests.rs"]
mod tests;
