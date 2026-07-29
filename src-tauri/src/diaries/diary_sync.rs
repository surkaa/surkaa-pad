use crate::caches::{LocalObjectEntry, LocalObjectStore};
use crate::local_storage::{available_space_for, required_space_with_margin};
use crate::object::{Object, OssClient};
use crate::storages::diary_id_from_manifest_key;
use crate::stream::ByteStream;
use crate::utils::message_sender::MessageSender;
use futures_util::StreamExt;
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::sync::Arc;
use tauri_plugin_log::log;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SyncDirection {
    Upload,
    Download,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SyncPhase {
    Preparing,
    Attachments,
    Manifests,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum SyncProgressEvent {
    Preparing {
        direction: SyncDirection,
    },
    Started {
        direction: SyncDirection,
        #[specta(rename = "totalFiles")]
        total_files: u32,
        #[specta(rename = "totalBytes", type = f64)]
        total_bytes: u64,
        #[specta(rename = "skippedFiles")]
        skipped_files: u32,
    },
    Progress {
        direction: SyncDirection,
        phase: SyncPhase,
        #[specta(rename = "currentFile")]
        current_file: String,
        #[specta(rename = "currentFileIndex")]
        current_file_index: u32,
        #[specta(rename = "totalFiles")]
        total_files: u32,
        #[specta(rename = "currentFileBytes", type = f64)]
        current_file_bytes: u64,
        #[specta(rename = "currentFileSize", type = f64)]
        current_file_size: u64,
        #[specta(rename = "transferredBytes", type = f64)]
        transferred_bytes: u64,
        #[specta(rename = "totalBytes", type = f64)]
        total_bytes: u64,
    },
    Completed {
        direction: SyncDirection,
        #[specta(rename = "transferredFiles")]
        transferred_files: u32,
        #[specta(rename = "skippedFiles")]
        skipped_files: u32,
        #[specta(rename = "transferredBytes", type = f64)]
        transferred_bytes: u64,
    },
    Error {
        direction: SyncDirection,
        phase: SyncPhase,
        #[specta(rename = "currentFile")]
        current_file: Option<String>,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncItemKind {
    Attachment,
    Manifest,
}

impl SyncItemKind {
    fn phase(self) -> SyncPhase {
        match self {
            Self::Attachment => SyncPhase::Attachments,
            Self::Manifest => SyncPhase::Manifests,
        }
    }

    fn order(self) -> u8 {
        match self {
            Self::Attachment => 0,
            Self::Manifest => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SyncObjectEntry {
    key: String,
    etag: Option<String>,
    size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SyncItem {
    key: String,
    diary_id: String,
    etag: Option<String>,
    size: u64,
    kind: SyncItemKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SyncPlan {
    items: Vec<SyncItem>,
    skipped_files: u32,
    skipped_bytes: u64,
    total_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloudToLocalSyncStats {
    pub remote_files: u32,
    pub remote_bytes: u64,
    pub download_files: u32,
    pub download_bytes: u64,
    pub skipped_files: u32,
    pub skipped_bytes: u64,
}

impl SyncPlan {
    fn stats(&self) -> CloudToLocalSyncStats {
        CloudToLocalSyncStats {
            remote_files: (self.items.len() as u32).saturating_add(self.skipped_files),
            remote_bytes: self.total_bytes.saturating_add(self.skipped_bytes),
            download_files: self.items.len() as u32,
            download_bytes: self.total_bytes,
            skipped_files: self.skipped_files,
            skipped_bytes: self.skipped_bytes,
        }
    }
}

fn ensure_download_capacity(plan: &SyncPlan, available_bytes: u64) -> Result<(), SyncFailure> {
    let required_bytes = required_space_with_margin(plan.total_bytes);
    if available_bytes >= required_bytes {
        return Ok(());
    }
    Err(SyncFailure::new(
        format!(
            "为避免下载后磁盘空间过低，当前无法下载：待下载 {}，本地可用 {}。请手动删除一些大附件或释放磁盘空间后重试",
            format_bytes(plan.total_bytes),
            format_bytes(available_bytes)
        ),
        SyncPhase::Preparing,
        None,
    ))
}

fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / GIB)
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MiB", bytes as f64 / MIB)
    } else {
        format!("{bytes} 字节")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SyncSummary {
    pub transferred_files: u32,
    pub skipped_files: u32,
    pub transferred_bytes: u64,
}

#[derive(Debug)]
pub struct SyncFailure {
    pub phase: SyncPhase,
    pub current_file: Option<String>,
    pub message: String,
}

impl SyncFailure {
    fn new(error: impl Display, phase: SyncPhase, current_file: Option<String>) -> Self {
        Self {
            phase,
            current_file,
            message: error.to_string(),
        }
    }

    pub fn into_event(self, direction: SyncDirection) -> SyncProgressEvent {
        SyncProgressEvent::Error {
            direction,
            phase: self.phase,
            current_file: self.current_file,
            message: self.message,
        }
    }
}

pub async fn sync_local_to_cloud(
    los: &LocalObjectStore,
    client: &OssClient,
    event: Arc<dyn MessageSender<SyncProgressEvent>>,
) -> Result<SyncSummary, SyncFailure> {
    let local_entries = los
        .get_all_entries()
        .await
        .map_err(|error| SyncFailure::new(error, SyncPhase::Preparing, None))?;
    let remote_entries = list_remote_objects(client).await?;
    let plan = build_plan(
        local_entries.into_iter().map(local_entry).collect(),
        &remote_entry_map(&remote_entries),
    );
    log::info!(
        "[sync] upload plan ready: transfer_files={}, skipped_files={}, total_bytes={}",
        plan.items.len(),
        plan.skipped_files,
        plan.total_bytes
    );
    send_started(&event, SyncDirection::Upload, &plan);

    let total_files = plan.items.len() as u32;
    let mut completed_files = 0u32;
    let mut transferred_bytes = 0u64;

    for item in &plan.items {
        let phase = item.kind.phase();
        let display_name = item.key.clone();
        log::info!(
            "[sync] uploading {}/{}: phase={:?}, key={}, size={}",
            completed_files + 1,
            total_files,
            phase,
            item.key,
            item.size
        );
        let stream = los
            .get_stream(&item.key, None)
            .await
            .map_err(|error| SyncFailure::new(error, phase, Some(display_name.clone())))?;
        let tracked_stream = track_sync_stream(
            stream,
            event.clone(),
            ProgressContext {
                direction: SyncDirection::Upload,
                phase,
                current_file: display_name.clone(),
                current_file_index: completed_files + 1,
                total_files,
                current_file_size: item.size,
                transferred_before: transferred_bytes,
                total_bytes: plan.total_bytes,
            },
        );
        let etag = client
            .upload(
                &item.key,
                item.size,
                tracked_stream,
                "application/octet-stream",
            )
            .await
            .map_err(|error| SyncFailure::new(error, phase, Some(display_name.clone())))?;
        los.set_etag(&item.key, &etag)
            .await
            .map_err(|error| SyncFailure::new(error, phase, Some(display_name.clone())))?;

        completed_files += 1;
        transferred_bytes = transferred_bytes.saturating_add(item.size);
        send_progress(
            &event,
            ProgressContext {
                direction: SyncDirection::Upload,
                phase,
                current_file: display_name,
                current_file_index: completed_files,
                total_files,
                current_file_size: item.size,
                transferred_before: transferred_bytes.saturating_sub(item.size),
                total_bytes: plan.total_bytes,
            },
            item.size,
        );
        log::info!(
            "[sync] uploaded {}/{}: key={}, etag={}",
            completed_files,
            total_files,
            item.key,
            etag
        );
    }

    Ok(SyncSummary {
        transferred_files: completed_files,
        skipped_files: plan.skipped_files,
        transferred_bytes,
    })
}

pub async fn inspect_cloud_to_local(
    los: &LocalObjectStore,
    client: &OssClient,
) -> Result<CloudToLocalSyncStats, SyncFailure> {
    Ok(cloud_to_local_plan(los, client).await?.stats())
}

async fn cloud_to_local_plan(
    los: &LocalObjectStore,
    client: &OssClient,
) -> Result<SyncPlan, SyncFailure> {
    let remote_entries = list_remote_objects(client).await?;
    let local_entries = los
        .get_all_entries()
        .await
        .map_err(|error| SyncFailure::new(error, SyncPhase::Preparing, None))?;
    Ok(build_plan(
        remote_entries.iter().map(remote_entry).collect(),
        &local_entry_map(&local_entries),
    ))
}

pub async fn sync_cloud_to_local(
    los: &LocalObjectStore,
    client: &OssClient,
    event: Arc<dyn MessageSender<SyncProgressEvent>>,
) -> Result<SyncSummary, SyncFailure> {
    let plan = cloud_to_local_plan(los, client).await?;
    let available_bytes = available_space_for(los.root())
        .map_err(|error| SyncFailure::new(error, SyncPhase::Preparing, None))?;
    ensure_download_capacity(&plan, available_bytes)?;
    log::info!(
        "[sync] download plan ready: transfer_files={}, skipped_files={}, total_bytes={}, available_bytes={}, required_bytes={}",
        plan.items.len(),
        plan.skipped_files,
        plan.total_bytes,
        available_bytes,
        required_space_with_margin(plan.total_bytes)
    );
    send_started(&event, SyncDirection::Download, &plan);

    let total_files = plan.items.len() as u32;
    let mut completed_files = 0u32;
    let mut transferred_bytes = 0u64;

    for item in &plan.items {
        let phase = item.kind.phase();
        let display_name = item.key.clone();
        log::info!(
            "[sync] downloading {}/{}: phase={:?}, key={}, size={}",
            completed_files + 1,
            total_files,
            phase,
            item.key,
            item.size
        );
        let etag = item.etag.as_deref().ok_or_else(|| {
            SyncFailure::new("云端对象缺少 ETag", phase, Some(display_name.clone()))
        })?;
        let (stream, _) = client
            .download(&item.key, None)
            .await
            .map_err(|error| SyncFailure::new(error, phase, Some(display_name.clone())))?;
        let tracked_stream = track_sync_stream(
            stream,
            event.clone(),
            ProgressContext {
                direction: SyncDirection::Download,
                phase,
                current_file: display_name.clone(),
                current_file_index: completed_files + 1,
                total_files,
                current_file_size: item.size,
                transferred_before: transferred_bytes,
                total_bytes: plan.total_bytes,
            },
        );
        los.save_stream_with_etag(&item.key, etag, tracked_stream)
            .await
            .map_err(|error| SyncFailure::new(error, phase, Some(display_name.clone())))?;

        completed_files += 1;
        transferred_bytes = transferred_bytes.saturating_add(item.size);
        send_progress(
            &event,
            ProgressContext {
                direction: SyncDirection::Download,
                phase,
                current_file: display_name,
                current_file_index: completed_files,
                total_files,
                current_file_size: item.size,
                transferred_before: transferred_bytes.saturating_sub(item.size),
                total_bytes: plan.total_bytes,
            },
            item.size,
        );
        log::info!(
            "[sync] downloaded {}/{}: key={}, etag={}",
            completed_files,
            total_files,
            item.key,
            etag
        );
    }

    Ok(SyncSummary {
        transferred_files: completed_files,
        skipped_files: plan.skipped_files,
        transferred_bytes,
    })
}

fn local_entry(entry: LocalObjectEntry) -> SyncObjectEntry {
    SyncObjectEntry {
        key: entry.key,
        etag: Some(entry.etag),
        size: entry.size,
    }
}

fn remote_entry(entry: &Object) -> SyncObjectEntry {
    SyncObjectEntry {
        key: entry.key.clone(),
        etag: entry.etag.clone(),
        size: entry.size,
    }
}

fn local_entry_map(entries: &[LocalObjectEntry]) -> HashMap<String, Option<String>> {
    entries
        .iter()
        .map(|entry| (entry.key.clone(), Some(entry.etag.clone())))
        .collect()
}

fn remote_entry_map(entries: &[Object]) -> HashMap<String, Option<String>> {
    entries
        .iter()
        .map(|entry| (entry.key.clone(), entry.etag.clone()))
        .collect()
}

fn build_plan(
    source_entries: Vec<SyncObjectEntry>,
    target_etags: &HashMap<String, Option<String>>,
) -> SyncPlan {
    let mut items = Vec::new();
    let mut skipped_files = 0u32;
    let mut skipped_bytes = 0u64;
    let source_diary_ids = source_entries
        .iter()
        .filter_map(|entry| match classify_storage_key(&entry.key) {
            Some((SyncItemKind::Manifest, diary_id)) => Some(diary_id),
            _ => None,
        })
        .collect::<HashSet<_>>();

    for entry in source_entries {
        let Some((kind, diary_id)) = classify_storage_key(&entry.key) else {
            continue;
        };
        if kind == SyncItemKind::Attachment && !source_diary_ids.contains(&diary_id) {
            continue;
        }
        if etag_options_match(entry.etag.as_deref(), target_etags.get(&entry.key)) {
            skipped_files += 1;
            skipped_bytes = skipped_bytes.saturating_add(entry.size);
            continue;
        }
        items.push(SyncItem {
            key: entry.key,
            diary_id,
            etag: entry.etag,
            size: entry.size,
            kind,
        });
    }

    items.sort_by(|left, right| {
        left.kind
            .order()
            .cmp(&right.kind.order())
            .then_with(|| left.key.cmp(&right.key))
    });
    let total_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size));
    SyncPlan {
        items,
        skipped_files,
        skipped_bytes,
        total_bytes,
    }
}

fn classify_storage_key(key: &str) -> Option<(SyncItemKind, String)> {
    if let Some(id) = diary_id_from_manifest_key(key) {
        return Some((SyncItemKind::Manifest, id));
    }
    if key.ends_with("/manifest.enc") {
        return None;
    }
    let (id, filename) = key.split_once('/')?;
    if id.is_empty() || filename.is_empty() || filename.contains('/') {
        return None;
    }
    Some((SyncItemKind::Attachment, id.to_string()))
}

fn etag_options_match(source: Option<&str>, target: Option<&Option<String>>) -> bool {
    match (source, target.and_then(Option::as_deref)) {
        (Some(source), Some(target)) => etags_match(source, target),
        _ => false,
    }
}

fn etags_match(left: &str, right: &str) -> bool {
    left.trim_matches('"')
        .eq_ignore_ascii_case(right.trim_matches('"'))
}

async fn list_remote_objects(client: &OssClient) -> Result<Vec<Object>, SyncFailure> {
    let mut entries = Vec::new();
    let mut next_token = None;
    loop {
        let (page, token) = client
            .list("", next_token)
            .await
            .map_err(|error| SyncFailure::new(error, SyncPhase::Preparing, None))?;
        entries.extend(page);
        if token.is_none() {
            break;
        }
        next_token = token;
    }
    Ok(entries)
}

fn send_started(
    event: &Arc<dyn MessageSender<SyncProgressEvent>>,
    direction: SyncDirection,
    plan: &SyncPlan,
) {
    let _ = event.send(SyncProgressEvent::Started {
        direction,
        total_files: plan.items.len() as u32,
        total_bytes: plan.total_bytes,
        skipped_files: plan.skipped_files,
    });
}

#[derive(Clone)]
struct ProgressContext {
    direction: SyncDirection,
    phase: SyncPhase,
    current_file: String,
    current_file_index: u32,
    total_files: u32,
    current_file_size: u64,
    transferred_before: u64,
    total_bytes: u64,
}

fn track_sync_stream(
    stream: ByteStream,
    event: Arc<dyn MessageSender<SyncProgressEvent>>,
    context: ProgressContext,
) -> ByteStream {
    let mut current_file_bytes = 0u64;
    let mut last_percentage = percentage(context.transferred_before, context.total_bytes);
    Box::pin(stream.inspect(move |result| {
        let Ok(bytes) = result else {
            return;
        };
        current_file_bytes = current_file_bytes.saturating_add(bytes.len() as u64);
        let transferred = context
            .transferred_before
            .saturating_add(current_file_bytes)
            .min(context.total_bytes);
        let current_percentage = percentage(transferred, context.total_bytes);
        if current_percentage > last_percentage {
            send_progress(&event, context.clone(), current_file_bytes);
            last_percentage = current_percentage;
        }
    }))
}

fn send_progress(
    event: &Arc<dyn MessageSender<SyncProgressEvent>>,
    context: ProgressContext,
    current_file_bytes: u64,
) {
    let transferred_bytes = context
        .transferred_before
        .saturating_add(current_file_bytes)
        .min(context.total_bytes);
    let _ = event.send(SyncProgressEvent::Progress {
        direction: context.direction,
        phase: context.phase,
        current_file: context.current_file,
        current_file_index: context.current_file_index,
        total_files: context.total_files,
        current_file_bytes: current_file_bytes.min(context.current_file_size),
        current_file_size: context.current_file_size,
        transferred_bytes,
        total_bytes: context.total_bytes,
    });
}

fn percentage(current: u64, total: u64) -> u8 {
    if total == 0 {
        100
    } else {
        ((current as u128 * 100 / total as u128).min(100)) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::LocalObjectStore;
    use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
    use crate::cryptos::Crypto;
    use crate::diaries::{DiaryContent, DiaryManifest};
    use crate::storages::remote_manifest_key;
    use crate::stream::collect_data;
    use crate::test_utils::TestOssGuard;
    use tokio::sync::mpsc::unbounded_channel;

    fn entry(key: &str, etag: Option<&str>, size: u64) -> SyncObjectEntry {
        SyncObjectEntry {
            key: key.to_string(),
            etag: etag.map(str::to_string),
            size,
        }
    }

    #[test]
    fn plan_skips_matching_etags_and_counts_transfer_bytes() {
        let source = vec![
            entry("123/manifest.enc", Some("MANIFEST"), 10),
            entry("123/photo.jpg", Some("PHOTO"), 1_000),
            entry("invalid", Some("IGNORED"), 99),
        ];
        let target = HashMap::from([
            ("123/manifest.enc".to_string(), Some("manifest".to_string())),
            ("123/photo.jpg".to_string(), Some("OLD".to_string())),
        ]);

        let plan = build_plan(source, &target);

        assert_eq!(plan.skipped_files, 1);
        assert_eq!(plan.skipped_bytes, 10);
        assert_eq!(plan.total_bytes, 1_000);
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].key, "123/photo.jpg");
        assert_eq!(
            plan.stats(),
            CloudToLocalSyncStats {
                remote_files: 2,
                remote_bytes: 1_010,
                download_files: 1,
                download_bytes: 1_000,
                skipped_files: 1,
                skipped_bytes: 10,
            }
        );
    }

    #[test]
    fn empty_local_source_does_not_touch_existing_remote_objects() {
        let remote = HashMap::from([
            ("123/manifest.enc".to_string(), Some("MANIFEST".to_string())),
            ("123/photo.jpg".to_string(), Some("PHOTO".to_string())),
        ]);

        let plan = build_plan(Vec::new(), &remote);

        assert!(plan.items.is_empty());
        assert_eq!(plan.total_bytes, 0);
        assert_eq!(plan.skipped_files, 0);
        assert_eq!(plan.skipped_bytes, 0);
    }

    #[test]
    fn plan_orders_attachments_before_manifests() {
        let source = vec![
            entry("200/manifest.enc", Some("1"), 1),
            entry("100/manifest.enc", Some("2"), 1),
            entry("200/2.jpg", Some("3"), 1),
            entry("100/1.jpg", Some("4"), 1),
        ];

        let plan = build_plan(source, &HashMap::new());
        let keys = plan
            .items
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            [
                "100/1.jpg",
                "200/2.jpg",
                "100/manifest.enc",
                "200/manifest.enc"
            ]
        );
    }

    #[test]
    fn plan_treats_missing_or_changed_target_as_transfer() {
        let source = vec![
            entry("123/manifest.enc", Some("MANIFEST"), 10),
            entry("123/a.jpg", Some("A"), 5),
            entry("123/b.jpg", None, 7),
        ];
        let target = HashMap::from([
            ("123/manifest.enc".to_string(), Some("MANIFEST".to_string())),
            ("123/a.jpg".to_string(), Some("B".to_string())),
        ]);

        let plan = build_plan(source, &target);

        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.total_bytes, 12);
        assert_eq!(plan.skipped_files, 1);
        assert_eq!(plan.skipped_bytes, 10);
    }

    #[test]
    fn plan_ignores_attachments_without_source_manifest() {
        let source = vec![
            entry("123/manifest.enc", Some("MANIFEST"), 10),
            entry("123/photo.jpg", Some("PHOTO"), 20),
            entry("orphan/photo.jpg", Some("ORPHAN_PHOTO"), 30),
            entry("orphan/audio.mp3", Some("ORPHAN_AUDIO"), 40),
        ];

        let plan = build_plan(source, &HashMap::new());
        let keys = plan
            .items
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(keys, ["123/photo.jpg", "123/manifest.enc"]);
        assert_eq!(plan.total_bytes, 30);
        assert_eq!(plan.skipped_files, 0);
        assert_eq!(plan.stats().remote_bytes, 30);
    }

    #[test]
    fn storage_key_classification_rejects_unrelated_paths() {
        assert_eq!(
            classify_storage_key("123/manifest.enc"),
            Some((SyncItemKind::Manifest, "123".to_string()))
        );
        assert_eq!(
            classify_storage_key("id/photo.jpg"),
            Some((SyncItemKind::Attachment, "id".to_string()))
        );
        assert_eq!(classify_storage_key("invalid"), None);
        assert_eq!(classify_storage_key("id/manifest.enc"), None);
        assert_eq!(
            classify_storage_key("rust-tests/run/123/manifest.enc"),
            None
        );
        assert_eq!(classify_storage_key("/photo.jpg"), None);
        assert_eq!(classify_storage_key("id/folder/photo.jpg"), None);
    }

    #[test]
    fn percentage_handles_empty_and_large_totals() {
        assert_eq!(percentage(0, 0), 100);
        assert_eq!(percentage(50, 100), 50);
        assert_eq!(percentage(u64::MAX, u64::MAX), 100);
    }

    #[test]
    fn download_capacity_blocks_below_safety_margin() {
        let plan = build_plan(
            vec![
                entry("123/manifest.enc", Some("MANIFEST"), 10),
                entry("123/video.mp4", Some("VIDEO"), 2 * 1024 * 1024),
            ],
            &HashMap::new(),
        );
        let required = required_space_with_margin(plan.total_bytes);

        assert!(ensure_download_capacity(&plan, required).is_ok());
        let error = ensure_download_capacity(&plan, required - 1).unwrap_err();
        assert_eq!(error.phase, SyncPhase::Preparing);
        assert!(error.message.contains("手动删除一些大附件"));
        assert!(error.message.contains(&format_bytes(plan.total_bytes)));
        assert!(!error.message.contains("安全余量"));
    }

    #[test]
    fn empty_download_plan_needs_no_safety_margin() {
        let plan = build_plan(Vec::new(), &HashMap::new());

        assert!(ensure_download_capacity(&plan, 0).is_ok());
    }

    #[test]
    fn progress_events_serialize_fields_as_camel_case() {
        let value = serde_json::to_value(SyncProgressEvent::Started {
            direction: SyncDirection::Upload,
            total_files: 2,
            total_bytes: 1024,
            skipped_files: 1,
        })
        .unwrap();

        assert_eq!(value["event"], "started");
        assert_eq!(value["data"]["totalFiles"], 2);
        assert_eq!(value["data"]["totalBytes"], 1024);
        assert_eq!(value["data"]["skippedFiles"], 1);
        assert!(value["data"].get("total_files").is_none());
    }

    #[tokio::test]
    async fn storage_sync_streams_roundtrip_and_skips_unchanged_files() {
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let (client, guard) = TestOssGuard::new(client).await;
        let source_dir = tempfile::tempdir().expect("source temp dir");
        let source_los = LocalObjectStore::new(source_dir.path().to_path_buf());
        let diary_id = "1234567890123";
        let attachment_key = format!("{diary_id}/large.bin");
        let manifest_key = remote_manifest_key(diary_id);
        let attachment = vec![0x5a; 256 * 1024];
        let manifest = DiaryManifest {
            id: diary_id.to_string(),
            algorithm: Gcm,
            content: DiaryContent::from_editor_text("同步测试标题\n正文"),
            created: 1,
            updated: 1,
            attachments: Vec::new(),
            version: crate::diaries::CURRENT_VERSION,
        };
        let encrypted_manifest = crypto
            .encrypt(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        source_los
            .save_bytes(&attachment_key, &attachment)
            .await
            .unwrap();
        source_los
            .save_bytes(&manifest_key, &encrypted_manifest)
            .await
            .unwrap();

        let (upload_tx, mut upload_rx) = unbounded_channel();
        let upload_summary = sync_local_to_cloud(&source_los, &client, Arc::new(upload_tx))
            .await
            .unwrap();
        let upload_events = std::iter::from_fn(|| upload_rx.try_recv().ok()).collect::<Vec<_>>();

        assert_eq!(upload_summary.transferred_files, 2);
        assert_eq!(upload_summary.skipped_files, 0);
        assert_eq!(
            upload_summary.transferred_bytes,
            attachment.len() as u64 + encrypted_manifest.len() as u64
        );
        let phases = upload_events
            .iter()
            .filter_map(|event| match event {
                SyncProgressEvent::Progress { phase, .. } => Some(*phase),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(phases.first(), Some(&SyncPhase::Attachments));
        assert_eq!(phases.last(), Some(&SyncPhase::Manifests));
        assert!(matches!(
            upload_events.last(),
            Some(SyncProgressEvent::Progress {
                transferred_bytes,
                total_bytes,
                ..
            }) if transferred_bytes == total_bytes
        ));

        let (retry_tx, _retry_rx) = unbounded_channel();
        let retry_summary = sync_local_to_cloud(&source_los, &client, Arc::new(retry_tx))
            .await
            .unwrap();
        assert_eq!(retry_summary.transferred_files, 0);
        assert_eq!(retry_summary.skipped_files, 2);

        let target_dir = tempfile::tempdir().expect("target temp dir");
        let target_los = LocalObjectStore::new(target_dir.path().to_path_buf());
        let (download_tx, mut download_rx) = unbounded_channel();
        let download_summary = sync_cloud_to_local(&target_los, &client, Arc::new(download_tx))
            .await
            .unwrap();
        let download_events =
            std::iter::from_fn(|| download_rx.try_recv().ok()).collect::<Vec<_>>();

        assert_eq!(download_summary.transferred_files, 2);
        assert_eq!(
            target_los.get_data(&attachment_key).await.unwrap(),
            attachment
        );
        assert_eq!(
            target_los.get_data(&manifest_key).await.unwrap(),
            encrypted_manifest
        );
        let downloaded_stream = target_los.get_stream(&attachment_key, None).await.unwrap();
        assert_eq!(
            collect_data(downloaded_stream).await.unwrap().len(),
            256 * 1024
        );
        assert!(matches!(
            download_events.last(),
            Some(SyncProgressEvent::Progress {
                transferred_bytes,
                total_bytes,
                ..
            }) if transferred_bytes == total_bytes
        ));

        guard.cleanup().await;
    }
}
