use crate::cryptos::Crypto;
use crate::diaries::diary::lock_diary_operation;
use crate::diaries::diary_migration::{migrate_manifest_bytes, MigrationContext};
use crate::diaries::diary_store::DiaryStore;
use crate::diaries::diary_types::{
    deserialize_current_manifest, inspect_manifest_json, DiaryManifest, CURRENT_VERSION,
};
use crate::diaries::DiaryError;
use crate::object::NextToken;
use crate::state::AppState;
use crate::utils::message_sender::MessageSender;
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri_plugin_log::log;
use tokio_util::sync::CancellationToken;

const FAILED_DIARY_ID_LIMIT: usize = 20;

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
    Failed,
    Upgraded,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiaryUpgradeIssue {
    pub diary_id: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiaryUpgradeSummary {
    pub upgraded: u32,
    pub skipped: u32,
    /// 只保留最多 20 项，避免大批损坏数据使事件消息无限增长。
    pub issues: Vec<DiaryUpgradeIssue>,
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
    pub failed_diary_ids: Vec<String>,
    pub upgrade: Option<DiaryUpgradeSummary>,
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
            failed_diary_ids: Vec::new(),
            upgrade: None,
        }
    }

    fn record_version(&mut self, version: u32) {
        self.processed_diaries = self.processed_diaries.saturating_add(1);
        if version < CURRENT_VERSION {
            self.legacy_diaries = self.legacy_diaries.saturating_add(1);
        } else if version == CURRENT_VERSION {
            self.current_diaries = self.current_diaries.saturating_add(1);
        } else {
            self.newer_diaries = self.newer_diaries.saturating_add(1);
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
        scope: DiaryVersionStorageScope,
        total: u32,
    },
    Progress {
        processed: u32,
        total: u32,
        #[specta(rename = "diaryId")]
        diary_id: String,
        outcome: DiaryVersionItemOutcome,
    },
    Completed {
        report: DiaryVersionReport,
    },
    Cancelled {
        report: DiaryVersionReport,
    },
    Error {
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
    let diary_ids = collect_diary_ids(store, cancellation).await?;
    let mut report = DiaryVersionReport::new(scope, diary_ids.len());
    let _ = event.send(DiaryVersionEvent::Started {
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

        let outcome = match inspect_diary_version(crypto, store, &diary_id).await {
            Ok(version) => {
                report.record_version(version);
                if version < CURRENT_VERSION {
                    DiaryVersionItemOutcome::Legacy
                } else if version == CURRENT_VERSION {
                    DiaryVersionItemOutcome::Current
                } else {
                    DiaryVersionItemOutcome::Newer
                }
            }
            Err(error) => {
                log::warn!("日记版本检查失败: diary={diary_id}, error={error}");
                report.record_failure(&diary_id);
                DiaryVersionItemOutcome::Failed
            }
        };

        let _ = event.send(DiaryVersionEvent::Progress {
            processed: report.processed_diaries,
            total: report.total_diaries,
            diary_id,
            outcome,
        });
    }

    Ok(DiaryVersionRunResult::Completed(report))
}

pub(crate) async fn upgrade_diary_versions(
    state: &AppState,
    scope: DiaryVersionStorageScope,
    event: Arc<dyn MessageSender<DiaryVersionEvent>>,
    cancellation: &CancellationToken,
) -> Result<DiaryVersionRunResult, DiaryError> {
    let reader = state.unoccupied_diary_store();
    let ids = collect_diary_ids(&*reader, cancellation).await?;
    let mut report = DiaryVersionReport::new(scope, ids.len());
    report.upgrade = Some(DiaryUpgradeSummary::default());
    let _ = event.send(DiaryVersionEvent::Started {
        scope,
        total: report.total_diaries,
    });
    for id in ids {
        if cancellation.is_cancelled() {
            return Ok(DiaryVersionRunResult::Cancelled(report));
        }
        let outcome = match upgrade_diary_version(state, &*reader, &id).await {
            Ok((version, outcome, issue)) => {
                report.record_version(version);
                let summary = report.upgrade.as_mut().expect("upgrade report");
                if outcome == DiaryVersionItemOutcome::Upgraded {
                    summary.upgraded += 1;
                }
                if outcome == DiaryVersionItemOutcome::Skipped {
                    summary.skipped += 1;
                }
                if let Some(message) = issue {
                    if summary.issues.len() < FAILED_DIARY_ID_LIMIT {
                        summary.issues.push(DiaryUpgradeIssue {
                            diary_id: id.clone(),
                            message,
                        });
                    }
                }
                outcome
            }
            Err(error) => {
                log::warn!("日记统一升级失败: diary={id}, error={error}");
                report.record_failure(&id);
                let summary = report.upgrade.as_mut().expect("upgrade report");
                if summary.issues.len() < FAILED_DIARY_ID_LIMIT {
                    summary.issues.push(DiaryUpgradeIssue {
                        diary_id: id.clone(),
                        message: error.to_string(),
                    });
                }
                DiaryVersionItemOutcome::Failed
            }
        };
        let _ = event.send(DiaryVersionEvent::Progress {
            processed: report.processed_diaries,
            total: report.total_diaries,
            diary_id: id,
            outcome,
        });
    }
    if cancellation.is_cancelled() {
        return Ok(DiaryVersionRunResult::Cancelled(report));
    }
    Ok(DiaryVersionRunResult::Completed(report))
}

async fn upgrade_diary_version(
    state: &AppState,
    reader: &dyn DiaryStore,
    id: &str,
) -> Result<(u32, DiaryVersionItemOutcome, Option<String>), DiaryError> {
    let _guard = lock_diary_operation(id).await;
    // 不使用可能已在内存中迁移的 DiaryCache，必须检查存储中的原始版本与 ETag。
    let (encrypted, baseline) = reader.download_manifest(id).await?;
    let crypto = state.crypto();
    let raw = crypto.decrypt(&encrypted)?;
    let (_, version) = inspect_manifest_json(id, &raw)?;
    if version == CURRENT_VERSION {
        deserialize_current_manifest(id, &raw)?;
        return Ok((version, DiaryVersionItemOutcome::Current, None));
    }
    if version > CURRENT_VERSION {
        return Ok((version, DiaryVersionItemOutcome::Newer, None));
    }
    let context = MigrationContext {
        diary_id: id,
        store: reader,
    };
    let migrated = match migrate_manifest_bytes(&context, &raw).await {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return Err(DiaryError::InvalidManifest(
                "旧版日记没有可执行的迁移步骤".into(),
            ))
        }
        Err(DiaryError::UnsupportedVersion { .. }) => {
            return Ok((
                version,
                DiaryVersionItemOutcome::Skipped,
                Some(format!("V{version} 没有可用的升级链，未修改原文件")),
            ))
        }
        Err(error) => return Err(error),
    };
    deserialize_current_manifest(id, &migrated)?;
    let encrypted = crypto.encrypt(&migrated)?;
    let edits = state.diary_edits();
    let token = if state.is_remote_enabled() {
        if edits
            .capture(id)
            .is_some_and(|permit| permit.check_local().is_ok())
        {
            return Ok((
                version,
                DiaryVersionItemOutcome::Skipped,
                Some("本机正在编辑此日记，请退出编辑后重试".into()),
            ));
        }
        // 不接管已过期或其他设备的占用；写入时仍由 RemoteStore 验证占用及原始 ETag。
        let access = edits.acquire(id, false, baseline).await?;
        if access.read_only {
            return Ok((version, DiaryVersionItemOutcome::Skipped, access.reason));
        }
        Some(
            access
                .session_id
                .ok_or_else(|| DiaryError::EditUnavailable("没有获得升级写入资格".into()))?,
        )
    } else {
        None
    };
    let store = state.diary_store();
    let result = store.upload_manifest(id, &encrypted).await;
    // 已经提交的单篇日记不回滚；取消只在单篇事务之间生效。失败也必须释放本次申请的占用。
    let release = if let Some(token) = token {
        edits.release(id, &token).await
    } else {
        Ok(())
    };
    result?;
    state.diary_cache().remove(id);
    let issue = release
        .err()
        .map(|error| format!("升级已完成，但释放编辑占用失败：{error}"));
    Ok((CURRENT_VERSION, DiaryVersionItemOutcome::Upgraded, issue))
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

async fn inspect_diary_version(
    crypto: &Crypto,
    store: &dyn DiaryStore,
    diary_id: &str,
) -> Result<u32, DiaryError> {
    let _guard = lock_diary_operation(diary_id).await;
    let (encrypted_manifest, _) = store.download_manifest(diary_id).await?;
    let manifest_bytes = crypto.decrypt(&encrypted_manifest)?;
    let (json, version) = inspect_manifest_json(diary_id, &manifest_bytes)?;

    // 当前版本必须真的符合当前 Rust 模型，不能只信任 version 数字。
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
#[path = "diary_upgrade_tests.rs"]
mod upgrade_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::LocalObjectStore;
    use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
    use crate::diaries::diary_content::DiaryContent;
    use crate::diaries::{DiaryStore, LocalStore};
    use tokio::sync::mpsc::unbounded_channel;

    pub(super) fn make_crypto() -> Crypto {
        let crypto = Crypto::new();
        crypto
            .derive_dek(
                "1".to_string(),
                "NFI2cXl3cUpiSDk4bVVkdEY4cDMzRzlqcTdMMkY5WDg",
            )
            .expect("derive test key");
        crypto
    }

    pub(super) fn current_manifest(id: &str) -> DiaryManifest {
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

    async fn upload_json(store: &LocalStore, crypto: &Crypto, id: &str, json: serde_json::Value) {
        let encrypted = crypto
            .encrypt(&serde_json::to_vec(&json).expect("serialize fixture"))
            .expect("encrypt fixture");
        store
            .upload_manifest(id, &encrypted)
            .await
            .expect("upload fixture");
    }

    pub(super) fn sender() -> (
        Arc<dyn MessageSender<DiaryVersionEvent>>,
        tokio::sync::mpsc::UnboundedReceiver<DiaryVersionEvent>,
    ) {
        let (tx, rx) = unbounded_channel();
        (Arc::new(tx), rx)
    }

    #[tokio::test]
    async fn inspection_is_read_only_and_classifies_supported_and_unsupported_versions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(temp_dir.path().to_path_buf()));
        let crypto = make_crypto();

        upload_json(
            &store,
            &crypto,
            "100",
            serde_json::to_value(current_manifest("100")).unwrap(),
        )
        .await;
        upload_json(
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
            serde_json::json!({"id":"400","version":CURRENT_VERSION}),
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
        assert_eq!(report.current_diaries, 1);
        assert_eq!(report.legacy_diaries, 1);
        assert_eq!(report.newer_diaries, 1);
        assert_eq!(report.failed_diaries, 1);
        assert_eq!(report.failed_diary_ids, vec!["400"]);

        let (encrypted, _) = store.download_manifest("200").await.unwrap();
        let raw = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(inspect_manifest_json("200", &raw).unwrap().1, 3);
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
