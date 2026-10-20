use super::*;
use crate::{
    caches::LocalObjectStore,
    cryptos::Crypto,
    diaries::{get_diary, save_diary, update_diary_content_only, RemoteStore},
    object::OssClient,
    object_locations::ObjectLocations,
    test_utils::TestOssGuard,
};

#[tokio::test]
async fn real_oss_read_only_and_editing_paths_preserve_data_and_drafts() {
    let client = OssClient::from_env();
    assert_eq!(std::env::var("ALIYUN_BUCKET_NAME").unwrap(), "surkaa-test");
    client.require_unversioned_bucket().await.unwrap();
    let (client, guard) = TestOssGuard::new(client).await;
    let temp = tempfile::tempdir().unwrap();
    let los = LocalObjectStore::new(temp.path().join("los"));
    let crypto = Crypto::new();
    crypto
        .derive_dek(
            "edit-integration-test".into(),
            "ZWRpdC1pbnRlZ3JhdGlvbi1zYWx0",
        )
        .unwrap();
    let state = AppState::from_parts(crypto.clone(), client.clone(), los.clone());
    state.set_remote_enabled(true);
    let raw = RemoteStore::new(los.clone(), client.clone());
    let (summary, _) = save_diary(&state.diary_cache(), &crypto, &raw, "original")
        .await
        .unwrap();
    let id = &summary.id;
    let key = ObjectLocations::diary_manifest(id);

    // 普通读取只在内存迁移；既不抢占，也不发布升级后的正文。
    let original = get_diary(&state.diary_cache(), &crypto, &raw, id)
        .await
        .unwrap();
    let mut old = serde_json::to_value(original).unwrap();
    old["version"] = 6.into();
    let old_bytes = crypto.encrypt(&serde_json::to_vec(&old).unwrap()).unwrap();
    let old_etag = client.upload_bytes(&key, &old_bytes).await.unwrap();
    let readonly_store = state.diary_store();
    assert_eq!(
        get_diary(&state.diary_cache(), &crypto, &*readonly_store, id)
            .await
            .unwrap()
            .version,
        7
    );
    assert_eq!(
        client.get_metadata(&key).await.unwrap().etag.unwrap(),
        old_etag
    );
    assert!(!client
        .object_exists(&ObjectLocations::diary_edit_lease(id))
        .await
        .unwrap());
    assert!(readonly_store.upload_manifest(id, b"bad").await.is_err());
    assert!(readonly_store.delete_diary_all(id).await.is_err());
    assert!(readonly_store
        .delete_attachment(id, "nonexistent")
        .await
        .is_err());
    assert!(readonly_store
        .begin_attachment_upload(id, "a", 1, "text/plain")
        .await
        .is_err());

    let view = open_edit_view(&state, id, false).await.unwrap();
    assert!(!view.access.read_only);
    let token = view.access.session_id.unwrap();
    let store = state.diary_store();
    let scoped = state.scoped_diary_writes();
    let draft = state
        .diary_drafts()
        .save(id, "new content".into())
        .await
        .unwrap();
    update_diary_content_only(&state.diary_cache(), &crypto, &*store, id, "new content")
        .await
        .unwrap();
    // 自己的连续写入推进同一基线，不会把正常的附件元信息更新误判为外部冲突。
    update_diary_content_only(&state.diary_cache(), &crypto, &*store, id, "second content")
        .await
        .unwrap();
    assert!(state
        .diary_drafts()
        .clear_if_revision(id, &draft)
        .await
        .unwrap());

    let mut upload = store
        .begin_attachment_upload(id, "pending", 1, "text/plain")
        .await
        .unwrap();
    upload.write_chunk(vec![1]).await.unwrap();
    // 同一设备重新打开也更换编辑令牌；旧任务和旧 multipart 不能借用新会话。
    let new_view = open_edit_view(&state, id, false).await.unwrap();
    assert_ne!(new_view.access.session_id.as_deref(), Some(token.as_str()));
    assert!(scoped
        .diary_store()
        .upload_manifest(id, b"stale")
        .await
        .is_err());
    assert!(upload.finish().await.is_err());
    assert!(!client
        .object_exists(&ObjectLocations::diary_attachment(id, "pending"))
        .await
        .unwrap());

    let _draft = state
        .diary_drafts()
        .save(id, "unsaved local".into())
        .await
        .unwrap();
    let foreign = client
        .upload_bytes(&key, b"changed by other client")
        .await
        .unwrap();
    let store = state.diary_store();
    assert!(store.upload_manifest(id, b"stale overwrite").await.is_err());
    assert_eq!(
        client.get_metadata(&key).await.unwrap().etag.unwrap(),
        foreign
    );
    assert_eq!(
        state
            .diary_drafts()
            .read(id)
            .await
            .unwrap()
            .unwrap()
            .content,
        DiaryContent::from("unsaved local")
    );
    client.delete(&key).await.unwrap();
    let recovery = open_edit_view(&state, id, false).await.unwrap();
    assert!(recovery.access.read_only);
    assert_eq!(recovery.detail.content, DiaryContent::from("unsaved local"));
    assert_eq!(
        state.diary_drafts().list_ids().await.unwrap(),
        vec![id.clone()]
    );
    assert!(recovery.draft.is_some());
    guard.cleanup().await;
}
