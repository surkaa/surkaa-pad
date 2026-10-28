use super::diary_content::{DiaryContent, SummaryContentNode};

#[test]
fn rejects_nested_summary_and_tracks_inner_attachments() {
    assert!(
        serde_json::from_value::<SummaryContentNode>(serde_json::json!({
            "type":"summary", "summary":"nested", "content":[]
        }))
        .is_err()
    );
    let mut content: DiaryContent = serde_json::from_value(serde_json::json!({"nodes":[{
        "type":"summary", "summary":"标题", "content":[
            {"type":"markdown","text":"内部正文"},
            {"type":"image","attachmentId":"image","size":"normal"},
            {"type":"album","id":"album","attachmentIds":["image","other"],"displayMode":"horizontalList"},
            {"type":"audio","attachmentId":"audio"},
            {"type":"video","attachmentId":"video"},
            {"type":"file","attachmentId":"file"}
        ]
    }]})).unwrap();
    assert!(content.searchable_text().contains("内部正文"));
    assert_eq!(content.title(), "标题");
    assert_eq!(content.attachment_counts().image, 3);
    assert_eq!(content.attachment_counts().audio, 1);
    assert_eq!(content.attachment_counts().video, 1);
    assert_eq!(content.attachment_counts().file, 1);
    content.remove_attachment("image");
    content.remove_attachment("file");
    assert_eq!(content.attachment_counts().image, 1);
    assert_eq!(content.attachment_counts().file, 0);
}

#[test]
fn blank_fold_title_falls_back_to_its_first_visible_text_heading() {
    let content: DiaryContent = serde_json::from_value(serde_json::json!({"nodes":[{
        "type":"summary", "summary":"", "content":[{"type":"markdown", "text":"## 内部标题\n\n正文"}]
    }]})).unwrap();
    assert_eq!(content.title(), "内部标题");
}

#[tokio::test]
async fn v7_migration_preserves_markdown_and_does_not_repeat_on_v8() {
    use super::{
        deserialize_current_manifest,
        diary_migration::{migrate_manifest_bytes, MigrationContext},
        LocalStore,
    };
    use crate::caches::LocalObjectStore;
    let temp = tempfile::tempdir().unwrap();
    let store = LocalStore::new(LocalObjectStore::new(temp.path().to_path_buf()));
    let context = MigrationContext {
        diary_id: "test",
        store: &store,
    };
    for text in [
        "",
        "## 标题\n\n**正文**\n\n| A | B |\n| --- | --- |\n| 1 | 2 |",
    ] {
        let source = serde_json::json!({"id":"test", "version":7, "algorithm":"AES256-GCM_v1", "created":1,"updated":2,
            "attachments":[], "content":{"nodes":[{"type":"summary","summary":"外显","content":text}]}});
        let result = migrate_manifest_bytes(&context, &serde_json::to_vec(&source).unwrap())
            .await
            .unwrap()
            .unwrap();
        let manifest = deserialize_current_manifest("test", &result).unwrap();
        assert_eq!(manifest.version, 8);
        assert!(manifest.content.searchable_text().contains(text));
        assert!(migrate_manifest_bytes(&context, &result)
            .await
            .unwrap()
            .is_none());
    }
}
