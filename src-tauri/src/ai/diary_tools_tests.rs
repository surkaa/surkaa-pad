use super::*;
use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
use crate::diaries::{save_diary, DiaryContent, DiaryContentNode};

fn tool_call(name: &str, arguments: Value) -> AiToolCall {
    AiToolCall {
        id: "call-1".into(),
        name: name.into(),
        arguments,
    }
}

#[test]
fn definitions_expose_only_three_read_only_tools() {
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
        [LIST_DIARIES_TOOL, SEARCH_DIARIES_TOOL, READ_DIARY_TOOL]
    );
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
            .execute(&tool_call(READ_DIARY_TOOL, json!({"diaryId": "../x"})))
            .await,
        Err(AiToolError::InvalidArguments { .. })
    ));
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
        }],
        version: 4,
    };

    let value = serde_json::to_value(DiaryToolDocument::from_manifest(&manifest)).unwrap();

    assert_eq!(value["content"], "正文\n[音频: 记录.m4a]\n");
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
        version: 4,
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
        version: 4,
    };

    let summary = DiaryToolSummary::from_manifest(&manifest);

    assert_eq!(summary.title.chars().count(), MAX_SUMMARY_TITLE_CHARS + 1);
    assert!(summary.title.ends_with('…'));
}
