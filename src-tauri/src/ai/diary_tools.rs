use super::{AiToolCall, AiToolCallDisplay, AiToolDefinition, AiToolError, AiToolExecutor};
use crate::attachments::AttachmentMeta;
use crate::diaries::{
    get_diary, DiaryAttachmentCounts, DiaryContentNode, DiaryManifest, DiarySummary,
};
use crate::object::NextToken;
use crate::state::AppState;
use async_trait::async_trait;
use chrono::{Local, NaiveDate, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

const LIST_DIARIES_TOOL: &str = "list_recent_diaries";
const LIST_DIARIES_BY_DATE_RANGE_TOOL: &str = "list_diaries_by_date_range";
const SEARCH_DIARIES_TOOL: &str = "search_diaries";
const READ_DIARY_TOOL: &str = "read_diary";
const DEFAULT_RESULT_LIMIT: usize = 10;
const MAX_RESULT_LIMIT: usize = 20;
const MAX_RESULT_PAGE: usize = 10_000;
const MAX_SUMMARY_TITLE_CHARS: usize = 200;
const MAX_DIARY_CONTENT_CHARS: usize = 60_000;
const MAX_TOOL_DISPLAY_CHARS: usize = 80;

#[derive(Clone)]
pub struct DiaryReadTools {
    state: AppState,
    storage_already_locked: bool,
}

impl DiaryReadTools {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            storage_already_locked: false,
        }
    }

    /// 供已经持有 `AppState::lock_storage_operation` 的长任务使用，避免公平读写锁中
    /// 有写者排队时再次获取读锁形成自锁等待。
    pub(crate) fn new_with_locked_storage(state: AppState) -> Self {
        Self {
            state,
            storage_already_locked: true,
        }
    }

    async fn list_recent_diaries(&self, limit: usize) -> Result<Value, AiToolError> {
        let _storage_guard = if self.storage_already_locked {
            None
        } else {
            Some(self.state.lock_storage_operation().await)
        };
        let cache = self.state.diary_cache();
        let crypto = self.state.crypto();
        let store = self.state.diary_store();
        let mut next_token: NextToken = None;
        let mut summaries = Vec::with_capacity(limit);

        while summaries.len() < limit {
            let (ids, next) = store
                .list_diary_ids(next_token)
                .await
                .map_err(|error| execution_failed(LIST_DIARIES_TOOL, error))?;
            for id in ids {
                let diary = get_diary(&cache, &crypto, &*store, &id)
                    .await
                    .map_err(|error| execution_failed(LIST_DIARIES_TOOL, error))?;
                summaries.push(DiaryToolSummary::from_manifest(&diary));
                if summaries.len() == limit {
                    break;
                }
            }
            if next.is_none() {
                break;
            }
            next_token = next;
        }

        Ok(json!({"diaries": summaries}))
    }

    async fn search_diaries(&self, query: &str, limit: usize) -> Result<Value, AiToolError> {
        let _storage_guard = if self.storage_already_locked {
            None
        } else {
            Some(self.state.lock_storage_operation().await)
        };
        let cache = self.state.diary_cache();
        let crypto = self.state.crypto();
        let store = self.state.diary_store();
        let mut next_token: NextToken = None;
        let mut summaries = Vec::with_capacity(limit);
        let keywords = query
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();

        while summaries.len() < limit {
            let (ids, next) = store
                .list_diary_ids(next_token)
                .await
                .map_err(|error| execution_failed(SEARCH_DIARIES_TOOL, error))?;
            for id in ids {
                let diary = get_diary(&cache, &crypto, &*store, &id)
                    .await
                    .map_err(|error| execution_failed(SEARCH_DIARIES_TOOL, error))?;
                if diary.matches_keywords(&keywords, false) {
                    summaries.push(DiaryToolSummary::from_manifest(&diary));
                    if summaries.len() == limit {
                        break;
                    }
                }
            }
            if next.is_none() {
                break;
            }
            next_token = next;
        }

        Ok(json!({"query": query, "diaries": summaries}))
    }

    async fn list_diaries_by_date_range(
        &self,
        range: DiaryDateRange,
        limit: usize,
        page: usize,
    ) -> Result<Value, AiToolError> {
        let _storage_guard = if self.storage_already_locked {
            None
        } else {
            Some(self.state.lock_storage_operation().await)
        };
        let cache = self.state.diary_cache();
        let crypto = self.state.crypto();
        let store = self.state.diary_store();
        let skip = page
            .checked_sub(1)
            .and_then(|page_index| page_index.checked_mul(limit))
            .ok_or_else(|| {
                invalid_arguments_for(LIST_DIARIES_BY_DATE_RANGE_TOOL, "分页范围过大")
            })?;
        let target_count = limit.saturating_add(1);
        let mut matched_count = 0;
        let mut summaries = Vec::with_capacity(target_count);
        let mut next_token: NextToken = None;

        'pages: loop {
            let (ids, next) = store
                .list_diary_ids(next_token)
                .await
                .map_err(|error| execution_failed(LIST_DIARIES_BY_DATE_RANGE_TOOL, error))?;
            for id in ids {
                let diary = get_diary(&cache, &crypto, &*store, &id)
                    .await
                    .map_err(|error| execution_failed(LIST_DIARIES_BY_DATE_RANGE_TOOL, error))?;
                if !range.contains_timestamp(diary.created) {
                    continue;
                }
                if matched_count < skip {
                    matched_count += 1;
                    continue;
                }
                summaries.push(DiaryToolSummary::from_manifest(&diary));
                if summaries.len() == target_count {
                    break 'pages;
                }
            }
            let Some(next) = next else {
                break;
            };
            next_token = Some(next);
        }

        let has_more = summaries.len() > limit;
        summaries.truncate(limit);
        Ok(json!({
            "startDate": range.start.to_string(),
            "endDate": range.end.to_string(),
            "page": page,
            "hasMore": has_more,
            "diaries": summaries,
        }))
    }

    async fn read_diary(&self, diary_id: &str) -> Result<Value, AiToolError> {
        let _storage_guard = if self.storage_already_locked {
            None
        } else {
            Some(self.state.lock_storage_operation().await)
        };
        let store = self.state.diary_store();
        let diary = get_diary(
            &self.state.diary_cache(),
            &self.state.crypto(),
            &*store,
            diary_id,
        )
        .await
        .map_err(|error| execution_failed(READ_DIARY_TOOL, error))?;

        serde_json::to_value(DiaryToolDocument::from_manifest(&diary))
            .map_err(|error| execution_failed(READ_DIARY_TOOL, error))
    }
}

#[async_trait]
impl AiToolExecutor for DiaryReadTools {
    fn definitions(&self) -> Vec<AiToolDefinition> {
        vec![
            AiToolDefinition {
                name: LIST_DIARIES_TOOL.into(),
                description: "按从新到旧顺序列出最近的日记摘要。需要了解最近写了什么或先浏览日记时使用。".into(),
                parameters: limit_schema(),
            },
            AiToolDefinition {
                name: LIST_DIARIES_BY_DATE_RANGE_TOOL.into(),
                description: "按设备本地日期范围列出日记摘要，起止日期均包含在内，结果从新到旧。需要查找较早时期或某段时间内的日记时使用。".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "startDate": {
                            "type": "string",
                            "description": "起始日期，格式为 YYYY-MM-DD",
                            "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$"
                        },
                        "endDate": {
                            "type": "string",
                            "description": "结束日期，格式为 YYYY-MM-DD",
                            "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$"
                        },
                        "limit": limit_property(),
                        "page": {
                            "type": "integer",
                            "description": "结果页码，从 1 开始",
                            "minimum": 1,
                            "maximum": MAX_RESULT_PAGE,
                            "default": 1
                        }
                    },
                    "required": ["startDate", "endDate"],
                    "additionalProperties": false
                }),
            },
            AiToolDefinition {
                name: SEARCH_DIARIES_TOOL.into(),
                description: "按正文或附件文件名搜索日记，空格分隔的多个关键词必须全部匹配。返回从新到旧的摘要。".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "要搜索的关键词；多个关键词用空格分隔",
                            "minLength": 1
                        },
                        "limit": limit_property()
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
            },
            AiToolDefinition {
                name: READ_DIARY_TOOL.into(),
                description: "读取指定日记的正文和附件占位说明。不会返回附件文件本身，也无法识别图片或播放音视频。".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "diaryId": {
                            "type": "string",
                            "description": "由日记列表或搜索工具返回的数字日记 ID",
                            "pattern": "^[0-9]+$"
                        }
                    },
                    "required": ["diaryId"],
                    "additionalProperties": false
                }),
            },
        ]
    }

    fn describe_call(&self, call: &AiToolCall) -> AiToolCallDisplay {
        match call.name.as_str() {
            LIST_DIARIES_TOOL => AiToolCallDisplay::new(
                "列出最近日记",
                call.arguments
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|limit| format!("最多 {limit} 篇")),
            ),
            LIST_DIARIES_BY_DATE_RANGE_TOOL => AiToolCallDisplay::new(
                "按日期浏览日记",
                match (
                    call.arguments.get("startDate").and_then(Value::as_str),
                    call.arguments.get("endDate").and_then(Value::as_str),
                ) {
                    (Some(start), Some(end)) => Some(format!("{start} 至 {end}")),
                    _ => None,
                },
            ),
            SEARCH_DIARIES_TOOL => AiToolCallDisplay::new(
                "搜索日记",
                call.arguments
                    .get("query")
                    .and_then(Value::as_str)
                    .map(compact_tool_display)
                    .filter(|query| !query.is_empty())
                    .map(|query| format!("“{query}”")),
            ),
            READ_DIARY_TOOL => AiToolCallDisplay::new(
                "读取日记",
                call.arguments
                    .get("diaryId")
                    .and_then(Value::as_str)
                    .map(compact_tool_display)
                    .filter(|id| !id.is_empty())
                    .map(|id| format!("日记 {id}")),
            ),
            _ => AiToolCallDisplay::new("执行未知日记操作", None),
        }
    }

    fn summarize_result(&self, call: &AiToolCall, result: Result<&Value, &AiToolError>) -> String {
        let value = match result {
            Ok(value) => value,
            Err(error) => return format!("操作失败：{}", error.display_reason()),
        };
        match call.name.as_str() {
            LIST_DIARIES_TOOL | LIST_DIARIES_BY_DATE_RANGE_TOOL | SEARCH_DIARIES_TOOL => value
                .get("diaries")
                .and_then(Value::as_array)
                .map(|diaries| format!("找到 {} 篇日记", diaries.len()))
                .unwrap_or_else(|| "日记查询完成".into()),
            READ_DIARY_TOOL => value
                .pointer("/summary/title")
                .and_then(Value::as_str)
                .map(compact_tool_display)
                .filter(|title| !title.is_empty())
                .map(|title| format!("已读取“{title}”"))
                .unwrap_or_else(|| "日记读取完成".into()),
            _ => "操作完成".into(),
        }
    }

    async fn execute(&self, call: &AiToolCall) -> Result<Value, AiToolError> {
        match call.name.as_str() {
            LIST_DIARIES_TOOL => {
                let args: LimitArgs = parse_arguments(call)?;
                self.list_recent_diaries(validate_limit(call, args.limit)?)
                    .await
            }
            LIST_DIARIES_BY_DATE_RANGE_TOOL => {
                let args: DateRangeArgs = parse_arguments(call)?;
                let range = DiaryDateRange::parse(call, &args.start_date, &args.end_date)?;
                let limit = validate_limit(call, args.limit)?;
                let page = validate_page(call, args.page)?;
                self.list_diaries_by_date_range(range, limit, page).await
            }
            SEARCH_DIARIES_TOOL => {
                let args: SearchArgs = parse_arguments(call)?;
                let query = args.query.trim();
                if query.is_empty() {
                    return Err(invalid_arguments(call, "query 不能为空"));
                }
                self.search_diaries(query, validate_limit(call, args.limit)?)
                    .await
            }
            READ_DIARY_TOOL => {
                let args: ReadArgs = parse_arguments(call)?;
                let diary_id = args.diary_id.trim();
                if diary_id.is_empty() || !diary_id.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(invalid_arguments(call, "diaryId 必须是非空数字 ID"));
                }
                self.read_diary(diary_id).await
            }
            _ => Err(AiToolError::UnknownTool(call.name.clone())),
        }
    }
}

fn compact_tool_display(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_with_ellipsis(&compact, MAX_TOOL_DISPLAY_CHARS)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitArgs {
    #[serde(default = "default_result_limit")]
    limit: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    #[serde(default = "default_result_limit")]
    limit: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DateRangeArgs {
    start_date: String,
    end_date: String,
    #[serde(default = "default_result_limit")]
    limit: usize,
    #[serde(default = "default_result_page")]
    page: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DiaryDateRange {
    start: NaiveDate,
    end: NaiveDate,
}

impl DiaryDateRange {
    fn parse(call: &AiToolCall, start: &str, end: &str) -> Result<Self, AiToolError> {
        let parse_date = |field: &str, value: &str| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
                invalid_arguments(call, format!("{field} 必须是有效的 YYYY-MM-DD 日期"))
            })
        };
        let range = Self {
            start: parse_date("startDate", start)?,
            end: parse_date("endDate", end)?,
        };
        if range.start > range.end {
            return Err(invalid_arguments(call, "startDate 不能晚于 endDate"));
        }
        Ok(range)
    }

    fn contains_timestamp(self, timestamp_millis: i64) -> bool {
        Local
            .timestamp_millis_opt(timestamp_millis)
            .single()
            .is_some_and(|timestamp| {
                let date = timestamp.date_naive();
                date >= self.start && date <= self.end
            })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadArgs {
    diary_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiaryToolSummary {
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
    attachment_count: usize,
    attachment_total_size: u64,
    attachment_counts: DiaryAttachmentCounts,
}

impl DiaryToolSummary {
    fn from_manifest(manifest: &DiaryManifest) -> Self {
        let summary = DiarySummary::from_manifest(manifest);
        Self {
            id: summary.id,
            title: truncate_with_ellipsis(&summary.title, MAX_SUMMARY_TITLE_CHARS),
            created_at: format_timestamp(summary.created),
            updated_at: format_timestamp(summary.updated),
            attachment_count: summary.attachment_count,
            attachment_total_size: summary.attachment_total_size,
            attachment_counts: summary.attachment_counts,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiaryToolDocument {
    summary: DiaryToolSummary,
    content: String,
    content_truncated: bool,
}

impl DiaryToolDocument {
    fn from_manifest(manifest: &DiaryManifest) -> Self {
        let rendered = render_content(manifest);
        let (content, content_truncated) = truncate_chars(&rendered, MAX_DIARY_CONTENT_CHARS);
        Self {
            summary: DiaryToolSummary::from_manifest(manifest),
            content,
            content_truncated,
        }
    }
}

fn render_content(manifest: &DiaryManifest) -> String {
    let attachments = manifest
        .attachments
        .iter()
        .map(|attachment| (attachment.id.as_str(), attachment))
        .collect::<HashMap<_, _>>();
    let mut output = String::new();

    for node in &manifest.content.nodes {
        match node {
            DiaryContentNode::Markdown { text } => output.push_str(text),
            DiaryContentNode::Summary { summary, content } => {
                output.push_str(&format!("\n[折叠内容：{summary}]\n{content}\n"));
            }
            DiaryContentNode::Location { location } => {
                let place = location.place_name.as_deref().unwrap_or("未命名地点");
                let accuracy = location
                    .horizontal_accuracy_meters
                    .map(|accuracy| format!("，精度约 ±{accuracy:.0} 米"))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "\n[位置: {place}，WGS-84 {:.6}, {:.6}{accuracy}]\n",
                    location.latitude, location.longitude
                ));
            }
            DiaryContentNode::Image { attachment_id, .. } => {
                push_attachment_marker(&mut output, "图片", attachment_id, &attachments)
            }
            DiaryContentNode::Video { attachment_id } => {
                push_attachment_marker(&mut output, "视频", attachment_id, &attachments)
            }
            DiaryContentNode::Audio { attachment_id } => {
                push_attachment_marker(&mut output, "音频", attachment_id, &attachments)
            }
            DiaryContentNode::File { attachment_id } => {
                push_attachment_marker(&mut output, "文件", attachment_id, &attachments)
            }
            DiaryContentNode::Album { attachment_ids, .. } => {
                let filenames = attachment_ids
                    .iter()
                    .map(|id| attachment_name(id, &attachments))
                    .collect::<Vec<_>>()
                    .join("、");
                output.push_str(&format!("\n[图集: {filenames}]\n"));
            }
        }
    }
    output
}

fn push_attachment_marker(
    output: &mut String,
    kind: &str,
    attachment_id: &str,
    attachments: &HashMap<&str, &AttachmentMeta>,
) {
    output.push_str(&format!(
        "\n[{kind}: {}]\n",
        attachment_name(attachment_id, attachments)
    ));
}

fn attachment_name(attachment_id: &str, attachments: &HashMap<&str, &AttachmentMeta>) -> String {
    attachments
        .get(attachment_id)
        .map(|attachment| attachment.filename.clone())
        .unwrap_or_else(|| "附件信息缺失".into())
}

fn parse_arguments<T: for<'de> Deserialize<'de>>(call: &AiToolCall) -> Result<T, AiToolError> {
    serde_json::from_value(call.arguments.clone())
        .map_err(|error| invalid_arguments(call, error.to_string()))
}

fn validate_limit(call: &AiToolCall, limit: usize) -> Result<usize, AiToolError> {
    if (1..=MAX_RESULT_LIMIT).contains(&limit) {
        Ok(limit)
    } else {
        Err(invalid_arguments(
            call,
            format!("limit 必须在 1 到 {MAX_RESULT_LIMIT} 之间"),
        ))
    }
}

fn validate_page(call: &AiToolCall, page: usize) -> Result<usize, AiToolError> {
    if (1..=MAX_RESULT_PAGE).contains(&page) {
        Ok(page)
    } else {
        Err(invalid_arguments(
            call,
            format!("page 必须在 1 到 {MAX_RESULT_PAGE} 之间"),
        ))
    }
}

fn invalid_arguments(call: &AiToolCall, message: impl Into<String>) -> AiToolError {
    AiToolError::InvalidArguments {
        tool: call.name.clone(),
        message: message.into(),
    }
}

fn invalid_arguments_for(tool: &str, message: impl Into<String>) -> AiToolError {
    AiToolError::InvalidArguments {
        tool: tool.into(),
        message: message.into(),
    }
}

fn execution_failed(tool: &str, error: impl std::fmt::Display) -> AiToolError {
    AiToolError::ExecutionFailed {
        tool: tool.into(),
        message: error.to_string(),
    }
}

fn default_result_limit() -> usize {
    DEFAULT_RESULT_LIMIT
}

fn default_result_page() -> usize {
    1
}

fn limit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"limit": limit_property()},
        "additionalProperties": false
    })
}

fn limit_property() -> Value {
    json!({
        "type": "integer",
        "description": "最多返回的日记数量",
        "minimum": 1,
        "maximum": MAX_RESULT_LIMIT,
        "default": DEFAULT_RESULT_LIMIT
    })
}

fn format_timestamp(timestamp_millis: i64) -> String {
    Utc.timestamp_millis_opt(timestamp_millis)
        .single()
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_else(|| timestamp_millis.to_string())
}

fn truncate_chars(text: &str, limit: usize) -> (String, bool) {
    let mut chars = text.chars();
    let truncated = chars.by_ref().take(limit).collect::<String>();
    let was_truncated = chars.next().is_some();
    (truncated, was_truncated)
}

fn truncate_with_ellipsis(text: &str, limit: usize) -> String {
    let (mut truncated, was_truncated) = truncate_chars(text, limit);
    if was_truncated {
        truncated.push('…');
    }
    truncated
}

#[cfg(test)]
#[path = "diary_tools_tests.rs"]
mod tests;
