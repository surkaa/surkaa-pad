use super::entry::{
    extract_entry, ArchiveEntryPhase, ArchiveEntryProgress, EntryControl, EntryRequest,
    MAX_ENTRY_BYTES,
};
use super::preview_cache::{EncryptedPreviewFile, PreviewWriter};
use super::range_reader::{ArchiveRangeSource, DiaryAttachmentRangeSource, SeekableRangeReader};
use super::ArchivePreviewError;
use crate::{
    diaries::get_diary, error::AppError, object_locations::ObjectLocations, state::AppState,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    collections::HashMap,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{ipc::Channel, AppHandle, Manager, State};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const RESOURCE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchiveEntryTarget {
    pub diary_id: String,
    pub attachment_id: String,
    pub index: u32,
    pub path: String,
    #[specta(type = f64)]
    pub size: u64,
    pub revision: String,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreparedArchiveEntry {
    pub id: String,
    pub filename: String,
    pub mimetype: String,
    #[specta(type = f64)]
    pub size: u64,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum ArchiveEntryEvent {
    Started,
    Reading {
        #[specta(type = f64)]
        bytes: u64,
    },
    Progress {
        progress: ArchiveEntryProgress,
    },
    Completed {
        resource: PreparedArchiveEntry,
    },
    PasswordRequired {
        #[serde(rename = "invalidPassword")]
        invalid_password: bool,
    },
    Cancelled,
    Error {
        message: String,
    },
}

pub(crate) struct EntryResource {
    pub id: String,
    pub target: ArchiveEntryTarget,
    pub remote: bool,
    pub file: EncryptedPreviewFile,
    pub mimetype: String,
    pub filename: String,
}
impl EntryResource {
    pub fn read_range(
        self: &Arc<Self>,
        start: u64,
        length: usize,
    ) -> impl std::future::Future<Output = io::Result<Vec<u8>>> + Send {
        let resource = self.clone();
        async move {
            tokio::task::spawn_blocking(move || resource.file.read_range(start, length))
                .await
                .map_err(io_error)?
        }
    }
}
struct CachedEntry {
    resource: Arc<EntryResource>,
    used: Instant,
    pinned: bool,
    revoked: bool,
    external_until: Option<Instant>,
}
impl CachedEntry {
    fn valid(&self) -> bool {
        !self.revoked
            && self.external_until.map_or(
                self.pinned || self.used.elapsed() <= RESOURCE_TTL,
                |until| until > Instant::now(),
            )
    }
}
pub(crate) struct EntryPreviews {
    entries: Mutex<HashMap<String, CachedEntry>>,
    pub worker: Arc<Semaphore>,
}
impl Default for EntryPreviews {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            worker: Arc::new(Semaphore::new(1)),
        }
    }
}
impl EntryPreviews {
    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<String, CachedEntry>> {
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }
    pub fn clear(&self) {
        let mut entries = self.entries();
        for entry in entries.values_mut() {
            entry.revoked = true;
        }
        entries.retain(|_, entry| Arc::strong_count(&entry.resource) > 1);
    }
    pub fn expire(&self) {
        self.entries()
            .retain(|_, entry| entry.valid() || Arc::strong_count(&entry.resource) > 1);
    }
    pub fn get(&self, id: &str) -> Option<Arc<EntryResource>> {
        let mut entries = self.entries();
        let entry = entries.get(id)?;
        if !entry.valid() {
            if Arc::strong_count(&entry.resource) == 1 {
                entries.remove(id);
            }
            return None;
        }
        let entry = entries.get_mut(id)?;
        entry.used = Instant::now();
        Some(entry.resource.clone())
    }
    fn find(&self, target: &ArchiveEntryTarget, remote: bool) -> Option<Arc<EntryResource>> {
        let mut entries = self.entries();
        let entry = entries.values_mut().find(|entry| {
            let item = &entry.resource.target;
            entry.valid()
                && entry.used.elapsed() <= RESOURCE_TTL
                && entry.resource.remote == remote
                && item.diary_id == target.diary_id
                && item.attachment_id == target.attachment_id
                && item.revision == target.revision
                && item.index == target.index
                && item.path == target.path
                && item.size == target.size
        })?;
        entry.pinned = true;
        entry.used = Instant::now();
        Some(entry.resource.clone())
    }
    fn reserve(&self, size: u64) -> io::Result<()> {
        let mut entries = self.entries();
        entries.retain(|_, entry| entry.valid() || Arc::strong_count(&entry.resource) > 1);
        loop {
            let total: u64 = entries.values().map(|e| e.resource.file.length).sum();
            if total + size <= MAX_CACHE_BYTES && entries.len() < 32 {
                return Ok(());
            }
            let candidate = entries
                .iter()
                .filter(|(_, e)| {
                    !e.pinned
                        && e.external_until.is_none_or(|until| until <= Instant::now())
                        && Arc::strong_count(&e.resource) == 1
                })
                .min_by_key(|(_, e)| e.used)
                .map(|(id, _)| id.clone());
            let Some(id) = candidate else {
                return Err(io_error("预览缓存已满，请先关闭其他预览"));
            };
            entries.remove(&id);
        }
    }
    fn insert(&self, resource: EntryResource) -> Arc<EntryResource> {
        let resource = Arc::new(resource);
        self.entries().insert(
            resource.id.clone(),
            CachedEntry {
                resource: resource.clone(),
                used: Instant::now(),
                pinned: true,
                revoked: false,
                external_until: None,
            },
        );
        resource
    }
    pub fn release(&self, id: &str, remove: bool) {
        let mut entries = self.entries();
        if let Some(entry) = entries.get_mut(id) {
            entry.pinned = false;
            if remove
                && entry
                    .external_until
                    .is_none_or(|until| until <= Instant::now())
            {
                entry.revoked = true;
                if Arc::strong_count(&entry.resource) == 1 {
                    entries.remove(id);
                }
            }
        }
    }
    fn external(&self, id: &str) -> io::Result<()> {
        let mut entries = self.entries();
        let entry = entries.get_mut(id).ok_or_else(|| io_error("预览已失效"))?;
        entry.external_until = Some(Instant::now() + RESOURCE_TTL);
        Ok(())
    }
}

fn io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

pub(super) async fn source_revision(
    state: &AppState,
    diary_id: &str,
    attachment_id: &str,
) -> Result<String, ArchivePreviewError> {
    let key = ObjectLocations::diary_attachment(diary_id, attachment_id);
    if state.is_remote_enabled() {
        state
            .oss_client()
            .get_metadata(&key)
            .await
            .map_err(io_error)?
            .etag
            .filter(|value| !value.is_empty())
            .ok_or_else(|| io_error("压缩包缺少 ETag，无法安全预览").into())
    } else {
        state
            .local_object_store()
            .get(&key)
            .await
            .map_err(io_error)?
            .ok_or_else(|| io_error("压缩包不存在").into())
    }
}

#[tauri::command]
#[specta::specta]
pub fn cmd_prepare_archive_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    target: ArchiveEntryTarget,
    password: Option<String>,
    event: Channel<ArchiveEntryEvent>,
) -> Result<String, AppError> {
    if target.path.len() > 4096
        || target.revision.is_empty()
        || target.revision.len() > 256
        || target.size > MAX_ENTRY_BYTES
    {
        return Err(io_error("条目参数无效或超过 256 MiB 预览上限").into());
    }
    let root = app
        .path()
        .app_cache_dir()
        .map_err(io_error)?
        .join("archive-entry-previews");
    let state = state.inner().clone();
    Ok(state
        .clone()
        .task_pool()
        .spawn_cancelable(move |cancellation| async move {
            let _ = event.send(ArchiveEntryEvent::Started);
            let result = prepare_entry(
                &state,
                &target,
                password.map(Zeroizing::new),
                root,
                &event,
                cancellation.clone(),
            )
            .await;
            let terminal = match result {
                Ok(resource) => ArchiveEntryEvent::Completed { resource },
                Err(_) if cancellation.is_cancelled() => ArchiveEntryEvent::Cancelled,
                Err(ArchivePreviewError::PasswordRequired) => ArchiveEntryEvent::PasswordRequired {
                    invalid_password: false,
                },
                Err(ArchivePreviewError::InvalidPassword) => ArchiveEntryEvent::PasswordRequired {
                    invalid_password: true,
                },
                Err(error) => ArchiveEntryEvent::Error {
                    message: error.to_string(),
                },
            };
            let _ = event.send(terminal);
        }))
}

pub(crate) async fn prepare_entry(
    state: &AppState,
    target: &ArchiveEntryTarget,
    password: Option<Zeroizing<String>>,
    root: PathBuf,
    event: &Channel<ArchiveEntryEvent>,
    cancellation: CancellationToken,
) -> Result<PreparedArchiveEntry, ArchivePreviewError> {
    let server = state.attachment_server();
    let previews = server.entry_previews.clone();
    let _worker = tokio::select! { _ = cancellation.cancelled() => return Err(io_error("预览已取消").into()),
    result = previews.worker.clone().acquire_owned() => result.map_err(io_error)? };
    let _guard = state.lock_storage_operation().await.map_err(io_error)?;
    let _ = event.send(ArchiveEntryEvent::Progress {
        progress: ArchiveEntryProgress {
            phase: ArchiveEntryPhase::Preparing,
            processed_bytes: 0,
            total_bytes: 0,
        },
    });
    if source_revision(state, &target.diary_id, &target.attachment_id).await? != target.revision {
        return Err(io_error("压缩包已变化，请重新打开目录").into());
    }
    let remote = state.is_remote_enabled();
    if let Some(resource) = previews.find(target, remote) {
        return Ok(resource_info(&server, &resource));
    }
    previews.reserve(target.size)?;
    let store = state.diary_store();
    let diary = get_diary(
        &state.diary_cache(),
        &state.crypto(),
        &*store,
        &target.diary_id,
    )
    .await
    .map_err(io_error)?;
    let mut attachment = diary
        .attachments
        .into_iter()
        .find(|item| item.id == target.attachment_id)
        .ok_or_else(|| io_error("附件不存在"))?;
    attachment.etag = Some(target.revision.clone());
    let length = store
        .get_attachment_size(
            &target.diary_id,
            &target.attachment_id,
            attachment.etag.as_deref(),
        )
        .await
        .map_err(io_error)?;
    let runtime = tokio::runtime::Handle::current();
    let crypto = state.crypto();
    let target_copy = target.clone();
    let event_copy = event.clone();
    let worker_cancel = cancellation.clone();
    let mut worker = tokio::task::spawn_blocking(move || {
        let source = DiaryAttachmentRangeSource::new(
            runtime,
            store,
            crypto,
            target_copy.diary_id.clone(),
            attachment,
            worker_cancel.clone(),
        );
        let source = ObservedSource {
            inner: source,
            bytes: 0,
            event: event_copy.clone(),
        };
        let reader = SeekableRangeReader::new(source, length).with_fetch_limit(MAX_CACHE_BYTES);
        let mut writer = PreviewWriter::new(&root)?;
        let mut control = EntryControl::new(
            worker_cancel.clone(),
            Arc::new(move |progress| {
                let _ = event_copy.send(ArchiveEntryEvent::Progress { progress });
            }),
        );
        extract_entry(
            reader,
            &EntryRequest {
                index: target_copy.index as usize,
                path: &target_copy.path,
                size: target_copy.size,
                password: password.as_ref().map(|p| p.as_str()),
            },
            &mut writer,
            &mut control,
        )?;
        control.check()?;
        writer.finish().map_err(ArchivePreviewError::from)
    });
    let result = tokio::select! {
        result = &mut worker => result.map_err(io_error)??,
        _ = tokio::time::sleep(Duration::from_secs(300)) => {
            cancellation.cancel();
            let _ = worker.await;
            return Err(io_error("读取/解压超过 5 分钟，请下载后查看").into());
        }
    };
    if cancellation.is_cancelled() {
        return Err(io_error("预览已取消").into());
    }
    let _ = event.send(ArchiveEntryEvent::Progress {
        progress: ArchiveEntryProgress {
            phase: ArchiveEntryPhase::Verifying,
            processed_bytes: 0,
            total_bytes: 0,
        },
    });
    if result.length != target.size
        || source_revision(state, &target.diary_id, &target.attachment_id).await? != target.revision
    {
        return Err(io_error("压缩包或条目在读取期间发生变化，未发布预览").into());
    }
    let prefix = result.read_range(0, (result.length as usize).min(8192))?;
    let detected = infer::get(&prefix)
        .map(|kind| kind.mime_type().to_owned())
        .unwrap_or_default();
    if detected.contains("zip")
        || detected.contains("compressed")
        || detected == "application/x-tar"
    {
        return Err(io_error("暂不支持预览嵌套压缩包").into());
    }
    let declared = mime_from_path(&target.path);
    let mimetype = if declared != "application/octet-stream" || detected.is_empty() {
        declared.into()
    } else {
        detected
    };
    let mut random = [0; 32];
    getrandom::fill(&mut random).map_err(io_error)?;
    let resource = previews.insert(EntryResource {
        id: hex::encode(random),
        target: target.clone(),
        remote,
        filename: target
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&target.path)
            .into(),
        mimetype,
        file: result,
    });
    Ok(resource_info(&server, &resource))
}

struct ObservedSource {
    inner: DiaryAttachmentRangeSource,
    bytes: u64,
    event: Channel<ArchiveEntryEvent>,
}
impl ArchiveRangeSource for ObservedSource {
    fn read_range(&mut self, start: u64, end: u64) -> io::Result<Vec<u8>> {
        let result = self.inner.read_range(start, end)?;
        self.bytes += result.len() as u64;
        let _ = self
            .event
            .send(ArchiveEntryEvent::Reading { bytes: self.bytes });
        Ok(result)
    }
}
fn resource_info(
    server: &crate::attachments::AttachmentServerHandle,
    resource: &EntryResource,
) -> PreparedArchiveEntry {
    PreparedArchiveEntry {
        id: resource.id.clone(),
        filename: resource.filename.clone(),
        mimetype: resource.mimetype.clone(),
        size: resource.file.length,
        url: server.entry_url(&resource.id),
    }
}
fn mime_from_path(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" | "xhtml" => "text/html",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "jsonl" | "ndjson" => "application/jsonl",
        "md" | "markdown" => "text/markdown",
        "svg" => "image/svg+xml",
        "webm" => "video/webm",
        "mp4" | "m4v" => "video/mp4",
        "m4a" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

#[tauri::command]
#[specta::specta]
pub fn cmd_release_archive_entry(state: State<'_, AppState>, id: String, remove: bool) {
    state
        .attachment_server()
        .entry_previews
        .release(&id, remove);
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_open_archive_entry_html(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let _guard = state.lock_storage_operation().await?;
    let server = state.attachment_server();
    let resource = server
        .entry_previews
        .get(&id)
        .ok_or_else(|| io_error("预览已失效"))?;
    if resource.mimetype != "text/html" || resource.remote != state.is_remote_enabled() {
        return Err(io_error("不是可打开的 HTML 预览").into());
    }
    server.entry_previews.external(&id)?;
    app.opener()
        .open_url(format!("{}?view=html", server.entry_url(&id)), None::<&str>)
        .map_err(io_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn resource(root: &std::path::Path, id: &str) -> EntryResource {
        let mut writer = PreviewWriter::new(root).unwrap();
        writer.write_all(b"private").unwrap();
        EntryResource {
            id: id.into(),
            remote: false,
            filename: "test.txt".into(),
            mimetype: "text/plain".into(),
            file: writer.finish().unwrap(),
            target: ArchiveEntryTarget {
                diary_id: "diary".into(),
                attachment_id: "archive".into(),
                index: 0,
                path: "test.txt".into(),
                size: 7,
                revision: "v1".into(),
            },
        }
    }

    #[test]
    fn lru_does_not_evict_pinned_files_or_lose_track_of_in_flight_requests() {
        let root = tempfile::tempdir().unwrap();
        let registry = EntryPreviews::default();
        let file = registry.insert(resource(root.path(), "one"));
        assert!(registry.reserve(MAX_CACHE_BYTES).is_err());
        registry.release("one", true);
        assert!(registry.get("one").is_none(), "撤销令牌立即生效");
        assert!(
            registry.reserve(MAX_CACHE_BYTES).is_err(),
            "在途读取仍占用磁盘预算"
        );
        drop(file);
        registry.reserve(MAX_CACHE_BYTES).unwrap();
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        drop(registry.insert(resource(root.path(), "old")));
        registry.release("old", false);
        registry.reserve(MAX_CACHE_BYTES).unwrap();
        assert!(registry.get("old").is_none());
    }

    #[test]
    fn expiration_clears_only_unpinned_resources_and_external_tokens_have_a_deadline() {
        let root = tempfile::tempdir().unwrap();
        let registry = EntryPreviews::default();
        drop(registry.insert(resource(root.path(), "pinned")));
        drop(registry.insert(resource(root.path(), "old")));
        registry.release("old", false);
        registry.entries().get_mut("old").unwrap().used =
            Instant::now() - RESOURCE_TTL - Duration::from_secs(1);
        registry.expire();
        assert!(registry.get("pinned").is_some());
        assert!(registry.get("old").is_none());
        registry.external("pinned").unwrap();
        registry.release("pinned", true);
        assert!(
            registry.get("pinned").is_some(),
            "外部浏览器令牌不随父弹窗关闭失效"
        );
        registry.entries().get_mut("pinned").unwrap().external_until =
            Some(Instant::now() - Duration::from_secs(1));
        registry.expire();
        assert!(registry.get("pinned").is_none());
    }

    #[test]
    fn cache_reuse_requires_identical_source_index_path_size_and_storage_mode() {
        let root = tempfile::tempdir().unwrap();
        let registry = EntryPreviews::default();
        let file = registry.insert(resource(root.path(), "one"));
        let target = file.target.clone();
        drop(file);
        assert!(registry.find(&target, false).is_some());
        assert!(registry.find(&target, true).is_none());
        let mut changed = target.clone();
        changed.revision = "v2".into();
        assert!(registry.find(&changed, false).is_none());
        changed = target.clone();
        changed.index = 1;
        assert!(registry.find(&changed, false).is_none());
        changed = target.clone();
        changed.path = "other".into();
        assert!(registry.find(&changed, false).is_none());
        changed = target;
        changed.size = 8;
        assert!(registry.find(&changed, false).is_none());
        registry.clear();
        assert!(registry.get("one").is_none());
    }
}
