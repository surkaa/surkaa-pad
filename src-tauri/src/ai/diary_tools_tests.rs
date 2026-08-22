use super::*;
use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
use crate::diaries::{save_diary, DiaryContent, DiaryContentNode, CURRENT_VERSION};
use crate::utils::id_generate::generate_descending_id_with_timestamp;

fn tool_call(name: &str, arguments: Value) -> AiToolCall {
    AiToolCall {
        id: "call-1".into(),
        name: name.into(),
        arguments,
    }
}

#[test]
fn definitions_expose_only_four_read_only_tools() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = AppState::from_parts(
        crate::cryptos::Crypto::new(),
        crate::object::OssClient::new(),
        crate::caches::LocalObjectStore::new(temp_dir.path().to_path_buf()),
    );
    let names = DiaryReadTools::new(state)
        .definitions()
        .into_iter()
        .map(|tool| tool.name)
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        [
            LIST_DIARIES_TOOL,
            LIST_DIARIES_BY_DATE_RANGE_TOOL,
            SEARCH_DIARIES_TOOL,
            READ_DIARY_TOOL
        ]
    );
}

#[test]
fn describes_tool_calls_and_results_without_exposing_raw_payloads() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = AppState::from_parts(
        crate::cryptos::Crypto::new(),
        crate::object::OssClient::new(),
        crate::caches::LocalObjectStore::new(temp_dir.path().to_path_buf()),
    );
    let tools = DiaryReadTools::new(state);
    let search = tool_call(
        SEARCH_DIARIES_TOOL,
        json!({"query": "  今天\n下雨  ", "limit": 5}),
    );
    let search_display = tools.describe_call(&search);

    assert_eq!(search_display.title, "搜索日记");
    assert_eq!(search_display.detail.as_deref(), Some("“今天 下雨”"));
    assert_eq!(
        tools.summarize_result(&search, Ok(&json!({"diaries": [{}, {}]}))),
        "找到 2 篇日记"
    );

    let read = tool_call(READ_DIARY_TOOL, json!({"diaryId": "123"}));
    let read_display = tools.describe_call(&read);
    assert_eq!(read_display.title, "读取日记");
    assert_eq!(read_display.detail.as_deref(), Some("日记 123"));
    assert_eq!(
        tools.summarize_result(&read, Ok(&json!({"summary": {"title": " 测试\n日记 "}})),),
        "已读取“测试 日记”"
    );

    let private_error = AiToolError::ExecutionFailed {
        tool: READ_DIARY_TOOL.into(),
        message: "C:\\Users\\name\\private diary.enc not found".into(),
    };
    let failure = tools.summarize_result(&read, Err(&private_error));
    assert_eq!(failure, "操作失败，AI 将根据现有信息继续处理");
    assert!(!failure.contains("C:\\Users"));
    assert!(!failure.contains("private diary.enc"));
}

#[test]
fn truncates_tool_display_text_on_character_boundaries() {
    let compact = compact_tool_display(&format!("  {}\n尾部  ", "词".repeat(100)));

    assert_eq!(compact.chars().count(), MAX_TOOL_DISPLAY_CHARS + 1);
    assert!(compact.ends_with('…'));
}

#[tokio::test]
async fn rejects_unknown_tools_and_invalid_arguments_without_reading_storage() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = AppState::from_parts(
        crate::cryptos::Crypto::new(),
        crate::object::OssClient::new(),
        crate::caches::LocalObjectStore::new(temp_dir.path().to_path_buf()),
    );
    let tools = DiaryReadTools::new(state);

    assert_eq!(
        tools
            .execute(&tool_call("delete_diary", json!({"diaryId": "1"})))
            .await,
        Err(AiToolError::UnknownTool("delete_diary".into()))
    );
    assert!(matches!(
        tools
            .execute(&tool_call(LIST_DIARIES_TOOL, json!({"limit": 21})))
            .await,
        Err(AiToolError::InvalidArguments { .. })
    ));
    assert!(matches!(
        tools
            .execute(&tool_call(
                LIST_DIARIES_BY_DATE_RANGE_TOOL,
                json!({"startDate": "2026-02-30", "endDate": "2026-03-01"}),
            ))
            .await,
        Err(AiToolError::InvalidArguments { .. })
    ));
    assert!(matches!(
        tools
            .execute(&tool_call(
                LIST_DIARIES_BY_DATE_RANGE_TOOL,
                json!({"startDate": "2026-03-02", "endDate": "2026-03-01"}),
            ))
            .await,
        Err(AiToolError::InvalidArguments { .. })
    ));
    assert!(matches!(
        tools
            .execute(&tool_call(
                LIST_DIARIES_BY_DATE_RANGE_TOOL,
                json!({"startDate": "2026-03-01", "endDate": "2026-03-02", "page": 0}),
            ))
            .await,
        Err(AiToolError::InvalidArguments { .. })
    ));
    assert!(matches!(
        tools
            .execute(&tool_call(READ_DIARY_TOOL, json!({"diaryId": "../x"})))
            .await,
        Err(AiToolError::InvalidArguments { .. })
    ));
}

#[test]
fn date_range_uses_inclusive_local_calendar_dates() {
    let date = Local::now().date_naive();
    let start = Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .unwrap()
        .timestamp_millis();
    let end = Local
        .from_local_datetime(&date.and_hms_opt(23, 59, 59).unwrap())
        .single()
        .unwrap()
        .timestamp_millis();
    let range = DiaryDateRange {
        start: date,
        end: date,
    };

    assert!(range.contains_timestamp(start));
    assert!(range.contains_timestamp(end));
    assert!(!range.contains_timestamp(start - 1));
    assert!(!range.contains_timestamp(end + 1_000));
}

#[tokio::test]
async fn lists_searches_and_reads_real_local_diaries() {
    let temp_dir = tempfile::tempdir().unwrap();
    let crypto = crate::cryptos::Crypto::new();
    crypto
        .derive_dek(
            "test-password".into(),
            crate::cryptos::crypto_types::DERIVE_SALT,
        )
        .unwrap();
    let state = AppState::from_parts(
        crypto,
        crate::object::OssClient::new(),
        crate::caches::LocalObjectStore::new(temp_dir.path().to_path_buf()),
    );
    let store = state.diary_store();
    let (older, _) = save_diary(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        "上海 旅行记录",
    )
    .await
    .unwrap();
    let (newer, _) = save_diary(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        "今天只在家休息",
    )
    .await
    .unwrap();
    drop(store);
    let tools = DiaryReadTools::new(state);

    let recent = tools
        .execute(&tool_call(LIST_DIARIES_TOOL, json!({"limit": 1})))
        .await
        .unwrap();
    assert_eq!(recent["diaries"][0]["id"], newer.id);
    assert_eq!(recent["diaries"][0]["title"], "今天只在家休息");

    let matches = tools
        .execute(&tool_call(
            SEARCH_DIARIES_TOOL,
            json!({"query": "上海 旅行"}),
        ))
        .await
        .unwrap();
    assert_eq!(matches["diaries"].as_array().unwrap().len(), 1);
    assert_eq!(matches["diaries"][0]["id"], older.id);

    let document = tools
        .execute(&tool_call(READ_DIARY_TOOL, json!({"diaryId": older.id})))
        .await
        .unwrap();
    assert_eq!(document["content"], "上海 旅行记录");
    assert_eq!(document["contentTruncated"], false);
}

#[tokio::test]
async fn lists_diaries_by_inclusive_date_range_with_stable_pagination() {
    let temp_dir = tempfile::tempdir().unwrap();
    let crypto = crate::cryptos::Crypto::new();
    crypto
        .derive_dek(
            "test-password".into(),
            crate::cryptos::crypto_types::DERIVE_SALT,
        )
        .unwrap();
    let state = AppState::from_parts(
        crypto,
        crate::object::OssClient::new(),
        crate::caches::LocalObjectStore::new(temp_dir.path().to_path_buf()),
    );
    let timestamp = |year, month, day, hour| {
        Local
            .with_ymd_and_hms(year, month, day, hour, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis()
    };
    let outside = save_diary_at(&state, "范围之外", timestamp(2026, 3, 9, 23)).await;
    let oldest = save_diary_at(&state, "范围内最早", timestamp(2026, 3, 10, 0)).await;
    let middle = save_diary_at(&state, "范围内中间", timestamp(2026, 3, 10, 12)).await;
    let newest = save_diary_at(&state, "范围内最新", timestamp(2026, 3, 11, 23)).await;
    let tools = DiaryReadTools::new(state);

    let first_page = tools
        .execute(&tool_call(
            LIST_DIARIES_BY_DATE_RANGE_TOOL,
            json!({
                "startDate": "2026-03-10",
                "endDate": "2026-03-11",
                "limit": 2,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(first_page["page"], 1);
    assert_eq!(first_page["hasMore"], true);
    assert_eq!(first_page["diaries"][0]["id"], newest);
    assert_eq!(first_page["diaries"][1]["id"], middle);

    let second_page = tools
        .execute(&tool_call(
            LIST_DIARIES_BY_DATE_RANGE_TOOL,
            json!({
                "startDate": "2026-03-10",
                "endDate": "2026-03-11",
                "limit": 2,
                "page": 2,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(second_page["hasMore"], false);
    assert_eq!(second_page["diaries"].as_array().unwrap().len(), 1);
    assert_eq!(second_page["diaries"][0]["id"], oldest);
    assert!(second_page.to_string().contains("范围内最早"));
    assert!(!first_page.to_string().contains(&outside));
    assert!(!second_page.to_string().contains(&outside));
}

async fn save_diary_at(state: &AppState, title: &str, created_at: i64) -> String {
    let id = generate_descending_id_with_timestamp(created_at);
    let manifest = DiaryManifest {
        id: id.clone(),
        algorithm: Gcm,
        content: DiaryContent {
            nodes: vec![DiaryContentNode::Markdown { text: title.into() }],
        },
        created: created_at,
        updated: created_at,
        attachments: vec![],
        version: CURRENT_VERSION,
    };
    let encrypted = state
        .crypto()
        .encrypt(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    state
        .diary_store()
        .upload_manifest(&id, &encrypted)
        .await
        .unwrap();
    id
}

#[test]
fn renders_attachment_placeholders_without_storage_details() {
    let manifest = DiaryManifest {
        id: "1".into(),
        algorithm: Gcm,
        content: DiaryContent {
            nodes: vec![
                DiaryContentNode::Markdown {
                    text: "正文".into(),
                },
                DiaryContentNode::Summary {
                    summary: "展开说明".into(),
                    content: "折叠正文".into(),
                },
                DiaryContentNode::Audio {
                    attachment_id: "audio-id".into(),
                },
            ],
        },
        created: 1,
        updated: 2,
        attachments: vec![AttachmentMeta {
            id: "audio-id".into(),
            filename: "记录.m4a".into(),
            mimetype: "video/mp4".into(),
            size: 12,
            encrypted: true,
            nonce: vec![1, 2, 3],
            algorithm: Gcm,
            etag: Some("private-etag".into()),
            content_info: None,
        }],
        version: CURRENT_VERSION,
    };

    let value = serde_json::to_value(DiaryToolDocument::from_manifest(&manifest)).unwrap();

    assert_eq!(
        value["content"],
        "正文\n[折叠内容：展开说明]\n折叠正文\n\n[音频: 记录.m4a]\n"
    );
    let serialized = value.to_string();
    assert!(!serialized.contains("private-etag"));
    assert!(!serialized.contains("nonce"));
    assert!(!serialized.contains("video/mp4"));
}

#[test]
fn truncates_long_diary_content_on_character_boundaries() {
    let text = "日".repeat(MAX_DIARY_CONTENT_CHARS + 1);
    let manifest = DiaryManifest {
        id: "1".into(),
        algorithm: Gcm,
        content: DiaryContent {
            nodes: vec![DiaryContentNode::Markdown { text }],
        },
        created: 1,
        updated: 2,
        attachments: vec![],
        version: CURRENT_VERSION,
    };

    let document = DiaryToolDocument::from_manifest(&manifest);

    assert_eq!(document.content.chars().count(), MAX_DIARY_CONTENT_CHARS);
    assert!(document.content_truncated);
}

#[test]
fn truncates_long_summary_titles() {
    let manifest = DiaryManifest {
        id: "1".into(),
        algorithm: Gcm,
        content: DiaryContent {
            nodes: vec![DiaryContentNode::Markdown {
                text: "题".repeat(MAX_SUMMARY_TITLE_CHARS + 1),
            }],
        },
        created: 1,
        updated: 2,
        attachments: vec![],
        version: CURRENT_VERSION,
    };

    let summary = DiaryToolSummary::from_manifest(&manifest);

    assert_eq!(summary.title.chars().count(), MAX_SUMMARY_TITLE_CHARS + 1);
    assert!(summary.title.ends_with('…'));
}
