use super::*;
use crate::{
    caches::LocalObjectStore,
    diaries::{LocalStore, RemoteStore},
    object::OssClient,
    object_locations::ObjectLocations,
    test_utils::TestOssGuard,
};

fn fixture(id: &str, version: u32) -> serde_json::Value {
    let mut manifest = serde_json::to_value(super::tests::current_manifest(id)).unwrap();
    manifest["version"] = version.into();
    if version < CURRENT_VERSION {
        manifest["content"] =
            serde_json::json!({"nodes":[{"type":"summary","summary":"标题","content":"**正文**"}]});
    }
    manifest
}
async fn put(store: &dyn DiaryStore, crypto: &Crypto, id: &str, version: u32) -> Vec<u8> {
    let raw = crypto
        .encrypt(&serde_json::to_vec(&fixture(id, version)).unwrap())
        .unwrap();
    store.upload_manifest(id, &raw).await.unwrap();
    raw
}
async fn run(state: &AppState, cancellation: &CancellationToken) -> DiaryVersionRunResult {
    upgrade_diary_versions(
        state,
        if state.is_remote_enabled() {
            DiaryVersionStorageScope::Cloud
        } else {
            DiaryVersionStorageScope::Local
        },
        super::tests::sender().0,
        cancellation,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn local_upgrade_validates_before_write_preserves_times_and_skips_unsupported_data() {
    let temp = tempfile::tempdir().unwrap();
    let los = LocalObjectStore::new(temp.path().join("los"));
    let crypto = super::tests::make_crypto();
    let state = AppState::from_parts(crypto.clone(), OssClient::new(), los.clone());
    let store = LocalStore::new(los.clone());
    let current = put(&store, &crypto, "100", CURRENT_VERSION).await;
    put(&store, &crypto, "200", 7).await;
    let unsupported = put(&store, &crypto, "300", 3).await;
    let future = put(&store, &crypto, "400", CURRENT_VERSION + 1).await;
    store.upload_manifest("500", b"corrupt").await.unwrap();
    let malformed = crypto.encrypt(br#"{"id":"600","version":7}"#).unwrap();
    store.upload_manifest("600", &malformed).await.unwrap();
    // 假设列表已经在内存缓存里把它迁移过；统一升级仍须实际写回原存储。
    state.diary_cache().insert(
        "200",
        super::tests::current_manifest("200"),
        "cached".into(),
    );
    let DiaryVersionRunResult::Completed(report) = run(&state, &CancellationToken::new()).await
    else {
        panic!()
    };
    assert_eq!(
        (
            report.current_diaries,
            report.legacy_diaries,
            report.newer_diaries,
            report.failed_diaries
        ),
        (2, 1, 1, 2)
    );
    let summary = report.upgrade.unwrap();
    assert_eq!(
        (summary.upgraded, summary.skipped, summary.issues.len()),
        (1, 1, 3)
    );
    for (id, original) in [
        ("100", current),
        ("300", unsupported),
        ("400", future),
        ("600", malformed),
    ] {
        assert_eq!(store.download_manifest(id).await.unwrap().0, original);
    }
    let migrated = crypto
        .decrypt(&store.download_manifest("200").await.unwrap().0)
        .unwrap();
    let migrated = deserialize_current_manifest("200", &migrated).unwrap();
    assert_eq!(
        (migrated.created, migrated.updated, migrated.version),
        (1, 1, CURRENT_VERSION)
    );
    assert!(state.diary_cache().get("200").is_none());
    let stored = store.download_manifest("200").await.unwrap().0;
    let DiaryVersionRunResult::Completed(report) = run(&state, &CancellationToken::new()).await
    else {
        panic!()
    };
    assert_eq!(report.upgrade.unwrap().upgraded, 0);
    assert_eq!(store.download_manifest("200").await.unwrap().0, stored);
}

#[tokio::test]
async fn precancelled_upgrade_does_not_touch_data() {
    let temp = tempfile::tempdir().unwrap();
    let crypto = super::tests::make_crypto();
    let state = AppState::from_parts(
        crypto.clone(),
        OssClient::new(),
        LocalObjectStore::new(temp.path().join("los")),
    );
    let store = state.diary_store();
    let original = put(&*store, &crypto, "200", 7).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let DiaryVersionRunResult::Cancelled(report) = run(&state, &cancel).await else {
        panic!()
    };
    assert_eq!(report.processed_diaries, 0);
    assert_eq!(store.download_manifest("200").await.unwrap().0, original);
}

#[tokio::test]
async fn cancellation_between_files_is_resumable_and_issue_list_is_bounded() {
    struct CancelAfterOne(CancellationToken);
    impl MessageSender<DiaryVersionEvent> for CancelAfterOne {
        fn send(&self, event: DiaryVersionEvent) -> Result<(), String> {
            if matches!(event, DiaryVersionEvent::Progress { .. }) {
                self.0.cancel();
            }
            Ok(())
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let crypto = super::tests::make_crypto();
    let state = AppState::from_parts(
        crypto.clone(),
        OssClient::new(),
        LocalObjectStore::new(temp.path().join("los")),
    );
    let store = state.diary_store();
    for index in 100..151 {
        put(&*store, &crypto, &index.to_string(), 7).await;
    }
    let token = CancellationToken::new();
    let result = upgrade_diary_versions(
        &state,
        DiaryVersionStorageScope::Local,
        Arc::new(CancelAfterOne(token.clone())),
        &token,
    )
    .await
    .unwrap();
    let DiaryVersionRunResult::Cancelled(report) = result else {
        panic!()
    };
    assert_eq!(report.total_diaries, 51);
    assert_eq!(report.processed_diaries, 1);
    assert_eq!(report.upgrade.unwrap().upgraded, 1);
    let DiaryVersionRunResult::Completed(report) = run(&state, &CancellationToken::new()).await
    else {
        panic!()
    };
    assert_eq!(report.current_diaries, 51);
    assert_eq!(report.upgrade.unwrap().upgraded, 50);
    for index in 200..225 {
        put(&*store, &crypto, &index.to_string(), 3).await;
    }
    let DiaryVersionRunResult::Completed(report) = run(&state, &CancellationToken::new()).await
    else {
        panic!()
    };
    let summary = report.upgrade.unwrap();
    assert_eq!(summary.skipped, 25);
    assert_eq!(summary.issues.len(), 20);
}

#[tokio::test]
async fn real_oss_upgrade_respects_local_and_remote_occupancy_and_does_not_touch_attachments() {
    let client = OssClient::from_env();
    assert_eq!(std::env::var("ALIYUN_BUCKET_NAME").unwrap(), "surkaa-test");
    let (client, guard) = TestOssGuard::new(client).await;
    let temp = tempfile::tempdir().unwrap();
    let crypto = super::tests::make_crypto();
    let los = LocalObjectStore::new(temp.path().join("los"));
    let state = AppState::from_parts(crypto.clone(), client.clone(), los.clone());
    state.set_remote_enabled(true);
    let other = AppState::from_parts(
        crypto.clone(),
        client.clone(),
        LocalObjectStore::new(temp.path().join("other")),
    );
    other.set_remote_enabled(true);
    let raw = RemoteStore::new(los, client.clone());
    for id in ["100", "200", "300"] {
        put(&raw, &crypto, id, 7).await;
    }
    let (_, baseline) = raw.download_manifest("200").await.unwrap();
    let local = state
        .diary_edits()
        .acquire("200", false, baseline)
        .await
        .unwrap()
        .session_id
        .unwrap();
    let (_, baseline) = raw.download_manifest("300").await.unwrap();
    let remote = other
        .diary_edits()
        .acquire("300", false, baseline)
        .await
        .unwrap()
        .session_id
        .unwrap();
    let attachment = ObjectLocations::diary_attachment("100", "untouched");
    client
        .upload_bytes(&attachment, b"original attachment")
        .await
        .unwrap();
    let attachment_etag = client.get_metadata(&attachment).await.unwrap().etag;
    let DiaryVersionRunResult::Completed(report) = run(&state, &CancellationToken::new()).await
    else {
        panic!()
    };
    let summary = report.upgrade.unwrap();
    assert_eq!((summary.upgraded, summary.skipped), (1, 2));
    assert_eq!(
        client.get_metadata(&attachment).await.unwrap().etag,
        attachment_etag
    );
    assert!(
        !other
            .diary_edits()
            .acquire("100", false, raw.download_manifest("100").await.unwrap().1)
            .await
            .unwrap()
            .read_only
    );
    // 跳过本机占用没有替换或废除原来的 token。
    state
        .diary_edits()
        .require_session("200", Some(&local))
        .unwrap();
    state.diary_edits().release("200", &local).await.unwrap();
    other.diary_edits().release("300", &remote).await.unwrap();
    let DiaryVersionRunResult::Completed(report) = run(&state, &CancellationToken::new()).await
    else {
        panic!()
    };
    assert_eq!(report.upgrade.unwrap().upgraded, 2);
    assert_eq!(report.current_diaries, 3);
    guard.cleanup().await;
}
