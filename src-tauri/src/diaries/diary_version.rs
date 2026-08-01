use crate::caches::DiaryMemoryCache;
use crate::cryptos::Crypto;
use crate::diaries::diary::{get_diary_locked, lock_diary_operation};
use crate::diaries::diary_migration::CURRENT_VERSION;
use crate::diaries::diary_store::DiaryStore;
use crate::diaries::diary_types::DiaryManifest;
use crate::diaries::DiaryError;
use crate::object::NextToken;
use crate::utils::message_sender::MessageSender;
use serde::Serialize;
use serde_json::Value;
use specta::Type;
use std::sync::Arc;
use tauri_plugin_log::log;
use tokio_util::sync::CancellationToken;

const FAILED_DIARY_ID_LIMIT: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DiaryVersionOperation {
    Inspect,
    Upgrade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DiaryVersionStorageScope {
    Local,
    Cloud,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DiaryVersionItemOutcome {
    Current,
    Legacy,
    Newer,
    Upgraded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiaryVersionCount {
    pub version: u32,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiaryVersionReport {
    pub scope: DiaryVersionStorageScope,
    pub current_version: u32,
    pub total_diaries: u32,
    pub processed_diaries: u32,
    pub current_diaries: u32,
    pub legacy_diaries: u32,
    pub newer_diaries: u32,
    pub failed_diaries: u32,
    pub upgraded_diaries: u32,
    pub versions: Vec<DiaryVersionCount>,
    pub failed_diary_ids: Vec<String>,
}

impl DiaryVersionReport {
    fn new(scope: DiaryVersionStorageScope, total_diaries: usize) -> Self {
        Self {
            scope,
            current_version: CURRENT_VERSION,
            total_diaries: u32::try_from(total_diaries).unwrap_or(u32::MAX),
            processed_diaries: 0,
            current_diaries: 0,
            legacy_diaries: 0,
            newer_diaries: 0,
            failed_diaries: 0,
            upgraded_diaries: 0,
            versions: Vec::new(),
            failed_diary_ids: Vec::new(),
        }
    }

    fn record_version(&mut self, version: u32, upgraded: bool) {
        self.processed_diaries = self.processed_diaries.saturating_add(1);
        if version < CURRENT_VERSION {
            self.legacy_diaries = self.legacy_diaries.saturating_add(1);
        } else if version == CURRENT_VERSION {
            self.current_diaries = self.current_diaries.saturating_add(1);
        } else {
            self.newer_diaries = self.newer_diaries.saturating_add(1);
        }
        if upgraded {
            self.upgraded_diaries = self.upgraded_diaries.saturating_add(1);
        }

        if let Some(entry) = self
            .versions
            .iter_mut()
            .find(|entry| entry.version == version)
        {
            entry.count = entry.count.saturating_add(1);
        } else {
            self.versions.push(DiaryVersionCount { version, count: 1 });
            self.versions.sort_by_key(|entry| entry.version);
        }
    }

    fn record_failure(&mut self, diary_id: &str) {
        self.processed_diaries = self.processed_diaries.saturating_add(1);
        self.failed_diaries = self.failed_diaries.saturating_add(1);
        if self.failed_diary_ids.len() < FAILED_DIARY_ID_LIMIT {
            self.failed_diary_ids.push(diary_id.to_string());
        }
    }
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum DiaryVersionEvent {
    Started {
        operation: DiaryVersionOperation,
        scope: DiaryVersionStorageScope,
        total: u32,
    },
    Progress {
        operation: DiaryVersionOperation,
        processed: u32,
        total: u32,
        #[specta(rename = "diaryId")]
        diary_id: String,
        outcome: DiaryVersionItemOutcome,
    },
    Completed {
        operation: DiaryVersionOperation,
        report: DiaryVersionReport,
    },
    Cancelled {
        operation: DiaryVersionOperation,
        report: DiaryVersionReport,
    },
    Error {
        operation: DiaryVersionOperation,
        message: String,
    },
}

pub(crate) enum DiaryVersionRunResult {
    Completed(DiaryVersionReport),
    Cancelled(DiaryVersionReport),
}

pub(crate) async fn inspect_diary_versions(
    crypto: &Crypto,
    store: &dyn DiaryStore,
    scope: DiaryVersionStorageScope,
    event: Arc<dyn MessageSender<DiaryVersionEvent>>,
    cancellation: &CancellationToken,
) -> Result<DiaryVersionRunResult, DiaryError> {
    run_diary_version_operation(
        None,
        crypto,
        store,
        DiaryVersionOperation::Inspect,
        scope,
        event,
        cancellation,
    )
    .await
}

pub(crate) async fn upgrade_legacy_diaries(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    scope: DiaryVersionStorageScope,
    event: Arc<dyn MessageSender<DiaryVersionEvent>>,
    cancellation: &CancellationToken,
) -> Result<DiaryVersionRunResult, DiaryError> {
    run_diary_version_operation(
        Some(cache),
        crypto,
        store,
        DiaryVersionOperation::Upgrade,
        scope,
        event,
        cancellation,
    )
    .await
}

async fn run_diary_version_operation(
    cache: Option<&DiaryMemoryCache>,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    operation: DiaryVersionOperation,
    scope: DiaryVersionStorageScope,
    event: Arc<dyn MessageSender<DiaryVersionEvent>>,
    cancellation: &CancellationToken,
) -> Result<DiaryVersionRunResult, DiaryError> {
    let diary_ids = collect_diary_ids(store, cancellation).await?;
    let mut report = DiaryVersionReport::new(scope, diary_ids.len());
    let _ = event.send(DiaryVersionEvent::Started {
        operation,
        scope,
        total: report.total_diaries,
    });
    if cancellation.is_cancelled() {
        return Ok(DiaryVersionRunResult::Cancelled(report));
    }

    for diary_id in diary_ids {
        if cancellation.is_cancelled() {
            return Ok(DiaryVersionRunResult::Cancelled(report));
        }

        let result = process_diary_version(cache, crypto, store, operation, &diary_id).await;
        let outcome = match result {
            Ok((version, upgraded)) => {
                report.record_version(version, upgraded);
                if upgraded {
                    DiaryVersionItemOutcome::Upgraded
                } else if version < CURRENT_VERSION {
                    DiaryVersionItemOutcome::Legacy
                } else if version == CURRENT_VERSION {
                    DiaryVersionItemOutcome::Current
                } else {
                    DiaryVersionItemOutcome::Newer
                }
            }
            Err(error) => {
                log::warn!(
                    "日记版本操作失败: operation={operation:?}, diary={diary_id}, error={error}"
                );
                report.record_failure(&diary_id);
                DiaryVersionItemOutcome::Failed
            }
        };

        let _ = event.send(DiaryVersionEvent::Progress {
            operation,
            processed: report.processed_diaries,
            total: report.total_diaries,
            diary_id,
            outcome,
        });
    }

    Ok(DiaryVersionRunResult::Completed(report))
}

async fn collect_diary_ids(
    store: &dyn DiaryStore,
    cancellation: &CancellationToken,
) -> Result<Vec<String>, DiaryError> {
    let mut diary_ids = Vec::new();
    let mut next_token: NextToken = None;
    loop {
        if cancellation.is_cancelled() {
            break;
        }
        let (ids, next) = store.list_diary_ids(next_token).await?;
        diary_ids.extend(ids);
        if next.is_none() {
            break;
        }
        next_token = next;
    }
    Ok(diary_ids)
}

async fn process_diary_version(
    cache: Option<&DiaryMemoryCache>,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    operation: DiaryVersionOperation,
    diary_id: &str,
) -> Result<(u32, bool), DiaryError> {
    let guard = lock_diary_operation(diary_id).await;
    let (encrypted_manifest, _) = store.download_manifest(diary_id).await?;
    let manifest_bytes = crypto.decrypt(&encrypted_manifest)?;
    let version = inspect_manifest_version(diary_id, &manifest_bytes)?;

    if operation != DiaryVersionOperation::Upgrade || version >= CURRENT_VERSION {
        return Ok((version, false));
    }

    let cache =
        cache.ok_or_else(|| DiaryError::InvalidManifest("升级操作缺少日记内存缓存".to_string()))?;
    let migrated = get_diary_locked(cache, crypto, store, diary_id, &guard).await?;
    if migrated.version != CURRENT_VERSION {
        return Err(DiaryError::InvalidManifest(format!(
            "Diary {diary_id} migrated to unexpected version {}",
            migrated.version
        )));
    }
    Ok((CURRENT_VERSION, true))
}

fn inspect_manifest_version(diary_id: &str, manifest_bytes: &[u8]) -> Result<u32, DiaryError> {
    let json: Value = serde_json::from_slice(manifest_bytes)?;
    let manifest_id = json
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| DiaryError::InvalidManifest("Manifest diary id is missing".to_string()))?;
    if manifest_id != diary_id {
        return Err(DiaryError::InvalidManifest(format!(
            "Manifest diary id {manifest_id} does not match requested id {diary_id}"
        )));
    }

    let version = match json.get("version") {
        None => 1,
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|version| u32::try_from(version).ok())
            .filter(|version| *version > 0)
            .ok_or_else(|| {
                DiaryError::InvalidManifest("Manifest version must be a positive u32".to_string())
            })?,
        Some(_) => {
            return Err(DiaryError::InvalidManifest(
                "Manifest version must be an integer".to_string(),
            ));
        }
    };

    // “当前版本”必须真的能反序列化成当前模型，不能只依赖 version 字段。
    if version == CURRENT_VERSION {
        let manifest: DiaryManifest = serde_json::from_value(json)?;
        if manifest.version != CURRENT_VERSION {
            return Err(DiaryError::InvalidManifest(format!(
                "Manifest version changed while parsing: {}",
                manifest.version
            )));
        }
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::LocalObjectStore;
    use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
    use crate::diaries::diary_content::DiaryContent;
    use crate::diaries::{DiaryStore, LocalStore};
    use crate::stream::{collect_data, create_mock_stream};
    use tokio::sync::mpsc::unbounded_channel;

    fn make_crypto() -> Crypto {
        let crypto = Crypto::new();
        crypto
            .derive_dek(
                "1".to_string(),
                "NFI2cXl3cUpiSDk4bVVkdEY4cDMzRzlqcTdMMkY5WDg",
            )
            .expect("derive test key");
        crypto
    }

    fn current_manifest(id: &str) -> DiaryManifest {
        DiaryManifest {
            id: id.to_string(),
            algorithm: Gcm,
            content: DiaryContent::default(),
            created: 1,
            updated: 1,
            attachments: Vec::new(),
            version: CURRENT_VERSION,
        }
    }

    async fn upload_json(store: &LocalStore, crypto: &Crypto, id: &str, json: Value) -> String {
        let encrypted = crypto
            .encrypt(&serde_json::to_vec(&json).expect("serialize fixture"))
            .expect("encrypt fixture");
        store
            .upload_manifest(id, &encrypted)
            .await
            .expect("upload fixture")
    }

    fn sender() -> (
        Arc<dyn MessageSender<DiaryVersionEvent>>,
        tokio::sync::mpsc::UnboundedReceiver<DiaryVersionEvent>,
    ) {
        let (tx, rx) = unbounded_channel();
        (Arc::new(tx), rx)
    }

    #[test]
    fn manifest_version_requires_valid_ids_and_current_shape() {
        let current = serde_json::to_vec(&current_manifest("current")).unwrap();
        assert_eq!(
            inspect_manifest_version("current", &current).unwrap(),
            CURRENT_VERSION
        );
        assert_eq!(
            inspect_manifest_version("legacy", br#"{"id":"legacy"}"#).unwrap(),
            1
        );
        assert!(inspect_manifest_version("requested", br#"{"id":"other","version":3}"#).is_err());
        assert!(inspect_manifest_version("bad", br#"{"id":"bad","version":0}"#).is_err());
        assert!(inspect_manifest_version("bad", br#"{"id":"bad","version":"4"}"#).is_err());
        assert!(inspect_manifest_version("bad", br#"{"id":"bad","version":4}"#).is_err());
    }

    #[tokio::test]
    async fn inspection_is_read_only_and_classifies_all_versions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(temp_dir.path().to_path_buf()));
        let crypto = make_crypto();

        let current = serde_json::to_value(current_manifest("100")).unwrap();
        upload_json(&store, &crypto, "100", current).await;
        let legacy_etag = upload_json(
            &store,
            &crypto,
            "200",
            serde_json::json!({"id":"200","version":3}),
        )
        .await;
        upload_json(
            &store,
            &crypto,
            "300",
            serde_json::json!({"id":"300","version":CURRENT_VERSION + 1}),
        )
        .await;
        upload_json(
            &store,
            &crypto,
            "400",
            serde_json::json!({"id":"wrong","version":3}),
        )
        .await;

        let (event, _rx) = sender();
        let result = inspect_diary_versions(
            &crypto,
            &store,
            DiaryVersionStorageScope::Local,
            event,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let DiaryVersionRunResult::Completed(report) = result else {
            panic!("inspection unexpectedly cancelled");
        };

        assert_eq!(report.total_diaries, 4);
        assert_eq!(report.processed_diaries, 4);
        assert_eq!(report.current_diaries, 1);
        assert_eq!(report.legacy_diaries, 1);
        assert_eq!(report.newer_diaries, 1);
        assert_eq!(report.failed_diaries, 1);
        assert_eq!(report.failed_diary_ids, vec!["400"]);
        assert_eq!(
            store.get_manifest_etag("200").await.unwrap(),
            Some(legacy_etag)
        );
        let (encrypted, _) = store.download_manifest("200").await.unwrap();
        let raw = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(inspect_manifest_version("200", &raw).unwrap(), 3);
    }

    #[tokio::test]
    async fn upgrade_moves_v3_attachment_and_publishes_current_manifest() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(temp_dir.path().to_path_buf()));
        let crypto = make_crypto();
        let cache = DiaryMemoryCache::new();
        let diary_id = "500";
        let filename = "photo.jpg";
        let data = b"legacy-photo";
        store
            .upload_attachment(
                diary_id,
                filename,
                data.len() as u64,
                "image/jpeg",
                create_mock_stream(data.to_vec(), data.len()),
            )
            .await
            .unwrap();
        upload_json(
            &store,
            &crypto,
            diary_id,
            serde_json::json!({
                "id": diary_id,
                "algorithm": "AES256-GCM_v1",
                "content": {"nodes":[{"type":"image","filename":filename,"size":"normal"}]},
                "created": 1,
                "updated": 1,
                "attachments": [{
                    "filename": filename,
                    "mimetype": "image/jpeg",
                    "size": data.len(),
                    "encrypted": false,
                    "nonce": [],
                    "algorithm": "AES256-GCM_v1",
                    "etag": null
                }],
                "version": 3
            }),
        )
        .await;

        let (event, _rx) = sender();
        let result = upgrade_legacy_diaries(
            &cache,
            &crypto,
            &store,
            DiaryVersionStorageScope::Local,
            event,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let DiaryVersionRunResult::Completed(report) = result else {
            panic!("upgrade unexpectedly cancelled");
        };
        assert_eq!(report.current_diaries, 1);
        assert_eq!(report.legacy_diaries, 0);
        assert_eq!(report.upgraded_diaries, 1);

        let (encrypted, _) = store.download_manifest(diary_id).await.unwrap();
        let raw = crypto.decrypt(&encrypted).unwrap();
        let manifest: DiaryManifest = serde_json::from_slice(&raw).unwrap();
        assert_eq!(manifest.version, CURRENT_VERSION);
        let attachment_id = &manifest.attachments[0].id;
        assert!(store
            .download_attachment(diary_id, filename, None, None)
            .await
            .is_err());
        let stream = store
            .download_attachment(diary_id, attachment_id, None, None)
            .await
            .unwrap();
        assert_eq!(collect_data(stream).await.unwrap(), data);
    }

    #[tokio::test]
    async fn inspection_covers_multiple_store_pages_and_can_cancel() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(temp_dir.path().to_path_buf()));
        let crypto = make_crypto();
        for index in 0..51 {
            let id = format!("{index:013}");
            upload_json(
                &store,
                &crypto,
                &id,
                serde_json::to_value(current_manifest(&id)).unwrap(),
            )
            .await;
        }

        let (event, _rx) = sender();
        let result = inspect_diary_versions(
            &crypto,
            &store,
            DiaryVersionStorageScope::Local,
            event,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let DiaryVersionRunResult::Completed(report) = result else {
            panic!("inspection unexpectedly cancelled");
        };
        assert_eq!(report.total_diaries, 51);
        assert_eq!(report.current_diaries, 51);

        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let (event, _rx) = sender();
        let result = inspect_diary_versions(
            &crypto,
            &store,
            DiaryVersionStorageScope::Local,
            event,
            &cancellation,
        )
        .await
        .unwrap();
        let DiaryVersionRunResult::Cancelled(report) = result else {
            panic!("pre-cancelled inspection should cancel");
        };
        assert_eq!(report.processed_diaries, 0);
    }
}
