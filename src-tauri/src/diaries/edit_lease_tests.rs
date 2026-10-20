use super::*;
use std::sync::atomic::{AtomicI64, AtomicUsize};

#[derive(Default)]
struct MemoryStorage {
    record: Mutex<Option<LeaseRecord>>,
    etag: Mutex<String>,
    now: AtomicI64,
    fail: AtomicBool,
    writes: AtomicUsize,
    steal_on_write: AtomicBool,
}
#[async_trait]
impl LeaseStorage for MemoryStorage {
    async fn read(&self, _: &str) -> Result<(Option<LeaseRecord>, i64), DiaryError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(unavailable("模拟断网"));
        }
        Ok((
            self.record.lock().await.clone(),
            self.now.load(Ordering::SeqCst),
        ))
    }
    async fn write(&self, _: &str, record: &LeaseRecord) -> Result<(), DiaryError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        let mut record = record.clone();
        if self.steal_on_write.load(Ordering::SeqCst) {
            record.token = "b".repeat(32);
        }
        *self.record.lock().await = Some(record);
        Ok(())
    }
    async fn manifest_etag(&self, _: &str) -> Result<String, DiaryError> {
        Ok(self.etag.lock().await.clone())
    }
}
async fn setup() -> (
    Arc<MemoryStorage>,
    DiaryEditManager,
    DiaryEditManager,
    String,
) {
    let storage = Arc::new(MemoryStorage::default());
    *storage.etag.lock().await = "base".into();
    let a = DiaryEditManager::with_storage(storage.clone());
    let b = DiaryEditManager::with_storage(storage.clone());
    let token = a
        .acquire("123", false, "base".into())
        .await
        .unwrap()
        .session_id
        .unwrap();
    (storage, a, b, token)
}

#[tokio::test]
async fn acquires_missing_record_and_foreign_session_is_read_only() {
    let (_, a, b, token) = setup().await;
    assert!(a.capture("123").unwrap().check().await.is_ok());
    let access = b.acquire("123", true, "base".into()).await.unwrap();
    assert!(access.read_only);
    assert!(!access.can_take_over);
    assert!(a.require_session("123", Some(&token)).is_ok());
    assert!(a.require_session("123", Some("foreign")).is_err());
    assert!(a.require_session("123", None).is_err());
}

#[tokio::test]
async fn expiry_requires_manual_takeover_and_old_session_cannot_write_or_release_successor() {
    let (storage, a, b, token) = setup().await;
    let old = a.capture("123").unwrap();
    storage.now.store(LEASE_MILLIS, Ordering::SeqCst);
    let access = b.acquire("123", false, "base".into()).await.unwrap();
    assert!(access.read_only && access.can_take_over);
    let new = b.acquire("123", true, "base".into()).await.unwrap();
    assert!(!new.read_only);
    assert!(old.check().await.is_err());
    assert!(a.renew("123", &token).await.is_err());
    a.release("123", &token).await.unwrap();
    assert_eq!(
        storage.record.lock().await.as_ref().unwrap().token,
        new.session_id.unwrap()
    );
    assert!(b.capture("123").unwrap().check().await.is_ok());
}

#[tokio::test]
async fn normal_release_allows_reopen_without_takeover() {
    let (_, a, b, token) = setup().await;
    a.release("123", &token).await.unwrap();
    assert!(
        !b.acquire("123", false, "base".into())
            .await
            .unwrap()
            .read_only
    );
}

#[tokio::test]
async fn expired_own_session_also_requires_manual_takeover() {
    let (storage, a, _, _) = setup().await;
    storage.now.store(LEASE_MILLIS, Ordering::SeqCst);
    let access = a.acquire("123", false, "base".into()).await.unwrap();
    assert!(access.read_only && access.can_take_over);
}

#[tokio::test]
async fn renewal_extends_server_deadline_without_changing_baseline() {
    let (storage, a, _, token) = setup().await;
    storage.now.store(30_000, Ordering::SeqCst);
    a.renew("123", &token).await.unwrap();
    assert_eq!(
        storage.record.lock().await.as_ref().unwrap().expires_at,
        150_000
    );
    assert_eq!(
        a.capture("123").unwrap().check().await.unwrap().baseline,
        "base"
    );
}

#[tokio::test]
async fn network_failure_invalidates_until_explicit_reopen() {
    let (storage, a, _, token) = setup().await;
    storage.fail.store(true, Ordering::SeqCst);
    assert!(a.renew("123", &token).await.is_err());
    storage.fail.store(false, Ordering::SeqCst);
    assert!(a.capture("123").unwrap().check().await.is_err());
}

#[tokio::test]
async fn changed_manifest_and_deleted_lease_stop_saves() {
    let (storage, a, _, _) = setup().await;
    *storage.etag.lock().await = "foreign".into();
    assert!(a
        .capture("123")
        .unwrap()
        .check()
        .await
        .unwrap_err()
        .to_string()
        .contains("云端日记已变化"));
    let (storage, a, _, _) = setup().await;
    *storage.record.lock().await = None;
    assert!(a.capture("123").unwrap().check().await.is_err());
}

#[tokio::test]
async fn readback_must_confirm_own_token() {
    let storage = Arc::new(MemoryStorage::default());
    storage.steal_on_write.store(true, Ordering::SeqCst);
    let manager = DiaryEditManager::with_storage(storage.clone());
    assert!(
        manager
            .acquire("123", false, "base".into())
            .await
            .unwrap()
            .read_only
    );
    assert!(manager.capture("123").is_none());
}

#[tokio::test]
async fn local_timeout_cannot_be_revived_just_because_remote_clock_looks_valid() {
    let (_, a, _, token) = setup().await;
    a.capture("123")
        .unwrap()
        .session
        .data
        .lock()
        .await
        .confirmed = Instant::now() - Duration::from_millis(LEASE_MILLIS as u64);
    assert!(a.renew("123", &token).await.is_err());
}

#[tokio::test]
async fn captured_operation_cannot_borrow_new_session() {
    let (storage, a, _, _) = setup().await;
    let old = a.capture("123").unwrap();
    storage.now.store(LEASE_MILLIS, Ordering::SeqCst);
    let new = a.acquire("123", true, "base".into()).await.unwrap();
    assert!(!new.read_only);
    assert!(old.check().await.is_err());
    assert!(a.capture("123").unwrap().check().await.is_ok());
    old.invalidate();
    assert!(a.capture("123").unwrap().check().await.is_ok());
}

#[tokio::test]
async fn rejects_invalid_ids_without_touching_cloud() {
    let storage = Arc::new(MemoryStorage::default());
    let manager = DiaryEditManager::with_storage(storage.clone());
    for id in ["", "../1", "1/2", "abc"] {
        assert!(manager.acquire(id, false, "base".into()).await.is_err());
    }
    assert_eq!(storage.writes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn real_oss_edit_lease_lifecycle() {
    let client = OssClient::from_env();
    assert_eq!(
        std::env::var("ALIYUN_BUCKET_NAME").unwrap(),
        "surkaa-test",
        "只能使用测试桶"
    );
    client.require_unversioned_bucket().await.unwrap();
    let (client, guard) = crate::test_utils::TestOssGuard::new(client).await;
    let key = ObjectLocations::diary_manifest("123");
    let etag = client.upload_bytes(&key, b"snapshot").await.unwrap();
    let (bytes, snapshot_etag, now) = client.download_snapshot(&key).await.unwrap();
    assert_eq!(bytes, b"snapshot");
    assert_eq!(snapshot_etag, etag);
    assert!(now > 0);
    let a = DiaryEditManager::new(client.clone());
    let b = DiaryEditManager::new(client.clone());
    let token = a
        .acquire("123", false, etag.clone())
        .await
        .unwrap()
        .session_id
        .unwrap();
    assert!(
        b.acquire("123", true, etag.clone())
            .await
            .unwrap()
            .read_only
    );
    a.renew("123", &token).await.unwrap();
    assert!(a.capture("123").unwrap().check().await.is_ok());
    client
        .upload_bytes(&key, b"changed elsewhere")
        .await
        .unwrap();
    assert!(a.capture("123").unwrap().check().await.is_err());
    a.release("123", &token).await.unwrap();
    let new_etag = client.get_metadata(&key).await.unwrap().etag.unwrap();
    assert!(!b.acquire("123", false, new_etag).await.unwrap().read_only);
    guard.cleanup().await;
}
