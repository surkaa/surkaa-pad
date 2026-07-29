use crate::app_config::{LocalStorageLocation, PendingLocalStorageMigration};
use crate::caches::{LocalObjectEntry, LocalObjectStore};
use crate::error::AppError;
use crate::local_storage::LocalStorageManager;
use crate::state::AppState;
use crate::utils::message_sender::MessageSender;
use futures_util::StreamExt;
use serde::Serialize;
use specta::Type;
#[cfg(windows)]
use std::path::Component;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::State;
use tauri_plugin_log::log;
use thiserror::Error;

const MINIMUM_FREE_SPACE_MARGIN: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LocalStorageMigrationPhase {
    Preparing,
    Copying,
    Verifying,
    Switching,
    Cleaning,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum LocalStorageMigrationEvent {
    Preparing {
        #[specta(rename = "sourcePath")]
        source_path: String,
        #[specta(rename = "targetPath")]
        target_path: String,
    },
    Started {
        #[specta(rename = "totalFiles")]
        total_files: u32,
        #[specta(rename = "totalBytes", type = f64)]
        total_bytes: u64,
        #[specta(rename = "fastMove")]
        fast_move: bool,
    },
    Phase {
        phase: LocalStorageMigrationPhase,
    },
    Progress {
        phase: LocalStorageMigrationPhase,
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
        #[specta(rename = "processedBytes", type = f64)]
        processed_bytes: u64,
        #[specta(rename = "totalBytes", type = f64)]
        total_bytes: u64,
    },
    Completed {
        #[specta(rename = "targetPath")]
        target_path: String,
        #[specta(rename = "migratedFiles")]
        migrated_files: u32,
        #[specta(rename = "migratedBytes", type = f64)]
        migrated_bytes: u64,
        #[specta(rename = "cleanupWarning")]
        cleanup_warning: Option<String>,
    },
    Error {
        phase: LocalStorageMigrationPhase,
        #[specta(rename = "currentFile")]
        current_file: Option<String>,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalStorageInfo {
    pub current_path: String,
    pub configured_path: String,
    pub is_default: bool,
    pub legacy_migration_required: bool,
    pub migration_pending: bool,
    pub total_files: u32,
    #[specta(type = f64)]
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalStorageMigrationStatus {
    pub legacy_migration_required: bool,
    pub migration_pending: bool,
    pub unavailable_path: Option<String>,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalStorageMigrationPlan {
    pub source_path: String,
    pub target_path: String,
    pub total_files: u32,
    #[specta(type = f64)]
    pub total_bytes: u64,
    #[specta(type = f64)]
    pub available_bytes: u64,
    #[specta(type = f64)]
    pub required_bytes: u64,
    pub fast_move: bool,
}

#[derive(Debug, Error)]
enum LocalStorageMigrationError {
    #[error("本地存储正在执行其他操作，请等待上传或同步完成")]
    Busy,
    #[error("自定义本地存储位置仅支持 Windows")]
    UnsupportedPlatform,
    #[error("本地存储目录必须是绝对路径")]
    RelativePath,
    #[error("选择的目录不存在或不是文件夹: {0}")]
    InvalidBasePath(String),
    #[error("目标目录不能与当前目录相同，也不能互相嵌套")]
    OverlappingPath,
    #[error("目标对象目录已存在且不为空: {0}")]
    TargetNotEmpty(String),
    #[error("目标磁盘空间不足，需要 {required} 字节，可用 {available} 字节")]
    InsufficientSpace { required: u64, available: u64 },
    #[error("本地对象存储错误: {0}")]
    Store(#[from] crate::caches::CacheError),
    #[error("应用配置错误: {0}")]
    Config(#[from] crate::app_config::AppConfigError),
    #[error("文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("对象 {key} 迁移校验失败")]
    VerificationFailed { key: String },
}

impl From<LocalStorageMigrationError> for AppError {
    fn from(error: LocalStorageMigrationError) -> Self {
        Self {
            error_type: "local_storage_migration".into(),
            message: error.to_string(),
        }
    }
}

#[derive(Clone)]
struct MigrationRequest {
    location: LocalStorageLocation,
    target_root: PathBuf,
}

struct MigrationPlanInternal {
    request: MigrationRequest,
    source_root: PathBuf,
    entries: Vec<LocalObjectEntry>,
    available_bytes: u64,
    required_bytes: u64,
    fast_move: bool,
}

impl MigrationPlanInternal {
    fn total_bytes(&self) -> u64 {
        self.entries
            .iter()
            .fold(0u64, |total, entry| total.saturating_add(entry.size))
    }

    fn public(&self) -> LocalStorageMigrationPlan {
        LocalStorageMigrationPlan {
            source_path: display_path(&self.source_root),
            target_path: display_path(&self.request.target_root),
            total_files: self.entries.len() as u32,
            total_bytes: self.total_bytes(),
            available_bytes: self.available_bytes,
            required_bytes: self.required_bytes,
            fast_move: self.fast_move,
        }
    }
}

/// 获取当前本地对象存储位置和数据规模。
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_local_storage_info(
    state: State<'_, AppState>,
) -> Result<LocalStorageInfo, AppError> {
    let los = state.local_object_store();
    let entries = los.get_all_entries().await?;
    let manager = state.local_storage();
    Ok(LocalStorageInfo {
        current_path: display_path(los.root()),
        configured_path: display_path(&manager.configured_root()),
        is_default: matches!(manager.configured_location(), LocalStorageLocation::Default),
        legacy_migration_required: manager.is_legacy_root(los.root()),
        migration_pending: manager.pending_migration().is_some(),
        total_files: entries.len() as u32,
        total_bytes: entries
            .iter()
            .fold(0u64, |total, entry| total.saturating_add(entry.size)),
    })
}

/// 轻量检查启动时是否需要继续或执行本地存储迁移。
#[tauri::command]
#[specta::specta]
pub fn cmd_get_local_storage_migration_status(
    state: State<'_, AppState>,
) -> LocalStorageMigrationStatus {
    let manager = state.local_storage();
    let root = state.local_object_store();
    let unavailable_reason = manager.active_root_unavailable_reason(root.root());
    LocalStorageMigrationStatus {
        legacy_migration_required: manager.is_legacy_root(root.root()),
        migration_pending: manager.pending_migration().is_some(),
        unavailable_path: unavailable_reason
            .as_ref()
            .map(|_| display_path(root.root())),
        unavailable_reason,
    }
}

/// 预检查本地对象存储迁移。`base_path` 为空时表示迁移到默认位置。
#[tauri::command]
#[specta::specta]
pub async fn cmd_plan_local_storage_migration(
    state: State<'_, AppState>,
    base_path: Option<String>,
) -> Result<LocalStorageMigrationPlan, AppError> {
    let request = resolve_request(&state.local_storage(), base_path)?;
    Ok(build_plan(&state.local_object_store(), request)
        .await?
        .public())
}

/// 执行本地对象存储迁移。成功后前端应立即重启应用。
#[tauri::command]
#[specta::specta]
pub async fn cmd_migrate_local_storage(
    state: State<'_, AppState>,
    event: Channel<LocalStorageMigrationEvent>,
    base_path: Option<String>,
) -> Result<(), AppError> {
    let event: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(event);
    let result = migrate_local_storage(&state, event.clone(), base_path).await;
    if let Err(error) = &result {
        let _ = event.send(LocalStorageMigrationEvent::Error {
            phase: LocalStorageMigrationPhase::Preparing,
            current_file: None,
            message: error.to_string(),
        });
    }
    result.map_err(Into::into)
}

async fn migrate_local_storage(
    state: &AppState,
    event: Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
    base_path: Option<String>,
) -> Result<(), LocalStorageMigrationError> {
    let _transition_guard = state
        .try_lock_storage_mode_change()
        .ok_or(LocalStorageMigrationError::Busy)?;
    execute_migration(
        state.local_object_store(),
        state.local_storage(),
        event,
        base_path,
        true,
    )
    .await
}

async fn execute_migration(
    source: LocalObjectStore,
    manager: LocalStorageManager,
    event: Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
    base_path: Option<String>,
    allow_fast_move: bool,
) -> Result<(), LocalStorageMigrationError> {
    let (request, pending) = match manager.pending_migration() {
        Some(pending) => (
            MigrationRequest {
                location: pending.target_location().clone(),
                target_root: pending.target_root().to_path_buf(),
            },
            Some(pending),
        ),
        None => (resolve_request(&manager, base_path)?, None),
    };

    let plan = build_plan_for_resume(&source, request, pending.as_ref()).await?;
    let public_plan = plan.public();
    let _ = event.send(LocalStorageMigrationEvent::Preparing {
        source_path: public_plan.source_path.clone(),
        target_path: public_plan.target_path.clone(),
    });
    let _ = event.send(LocalStorageMigrationEvent::Started {
        total_files: public_plan.total_files,
        total_bytes: public_plan.total_bytes,
        fast_move: public_plan.fast_move,
    });

    if plan.source_root == plan.request.target_root {
        manager
            .config()
            .complete_local_storage_migration(plan.request.location.clone())?;
        send_completed(&event, &plan, None);
        return Ok(());
    }

    let staging_root = pending
        .as_ref()
        .map(|pending| pending.staging_root().to_path_buf())
        .unwrap_or_else(|| staging_root(&plan.request.target_root));
    if pending.is_none() {
        if staging_root.exists() {
            std::fs::remove_dir_all(&staging_root)?;
        }
        manager
            .config()
            .begin_local_storage_migration(PendingLocalStorageMigration::new(
                plan.source_root.clone(),
                plan.request.target_root.clone(),
                staging_root.clone(),
                plan.request.location.clone(),
            ))?;
    }

    if !plan.source_root.exists() && plan.request.target_root.exists() {
        manager
            .config()
            .complete_local_storage_migration(plan.request.location.clone())?;
        send_completed(&event, &plan, None);
        return Ok(());
    }

    if plan.entries.is_empty() && !plan.source_root.exists() {
        std::fs::create_dir_all(&plan.request.target_root)?;
        manager
            .config()
            .complete_local_storage_migration(plan.request.location.clone())?;
        send_completed(&event, &plan, None);
        return Ok(());
    }

    if allow_fast_move && plan.fast_move {
        if let Some(parent) = plan.request.target_root.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if plan.request.target_root.exists() {
            std::fs::remove_dir_all(&plan.request.target_root)?;
        }
        let _ = event.send(LocalStorageMigrationEvent::Phase {
            phase: LocalStorageMigrationPhase::Switching,
        });
        match std::fs::rename(&plan.source_root, &plan.request.target_root) {
            Ok(()) => {
                manager
                    .config()
                    .complete_local_storage_migration(plan.request.location.clone())?;
                send_completed(&event, &plan, None);
                return Ok(());
            }
            Err(error) => {
                log::warn!(
                    "[local storage] fast move failed, falling back to streaming copy: {error}"
                );
            }
        }
    }

    if plan.request.target_root.exists() && directory_has_entries(&plan.request.target_root)? {
        verify_entries(
            &source,
            &LocalObjectStore::new(plan.request.target_root.clone()),
            &plan.entries,
            event.clone(),
        )
        .await?;
    } else {
        let available = available_space_for(&plan.request.target_root)?;
        if available < plan.required_bytes {
            return Err(LocalStorageMigrationError::InsufficientSpace {
                required: plan.required_bytes,
                available,
            });
        }
        let staging = LocalObjectStore::new(staging_root.clone());
        copy_entries(&source, &staging, &plan.entries, event.clone()).await?;
        verify_entries(&source, &staging, &plan.entries, event.clone()).await?;

        if plan.request.target_root.exists() {
            std::fs::remove_dir_all(&plan.request.target_root)?;
        }
        if let Some(parent) = plan.request.target_root.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = event.send(LocalStorageMigrationEvent::Phase {
            phase: LocalStorageMigrationPhase::Switching,
        });
        std::fs::rename(&staging_root, &plan.request.target_root)?;
    }

    manager
        .config()
        .complete_local_storage_migration(plan.request.location.clone())?;
    let _ = event.send(LocalStorageMigrationEvent::Phase {
        phase: LocalStorageMigrationPhase::Cleaning,
    });
    let cleanup_warning = match std::fs::remove_dir_all(&plan.source_root) {
        Ok(()) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => Some(format!(
            "新位置已启用，但旧目录清理失败：{} ({error})",
            display_path(&plan.source_root)
        )),
    };
    send_completed(&event, &plan, cleanup_warning);
    Ok(())
}

async fn build_plan(
    source: &LocalObjectStore,
    request: MigrationRequest,
) -> Result<MigrationPlanInternal, LocalStorageMigrationError> {
    build_plan_for_resume(source, request, None).await
}

async fn build_plan_for_resume(
    source: &LocalObjectStore,
    request: MigrationRequest,
    pending: Option<&PendingLocalStorageMigration>,
) -> Result<MigrationPlanInternal, LocalStorageMigrationError> {
    let source_root = source.root().to_path_buf();
    validate_path_relationship(&source_root, &request.target_root)?;
    let entries = source.get_all_entries().await?;
    if source_root == request.target_root {
        return Ok(MigrationPlanInternal {
            request,
            source_root,
            entries,
            available_bytes: available_space_for(source.root())?,
            required_bytes: 0,
            fast_move: true,
        });
    }
    let target_nonempty =
        request.target_root.exists() && directory_has_entries(&request.target_root)?;
    let resuming_completed_target = pending.is_some() && target_nonempty;
    if target_nonempty && !resuming_completed_target {
        return Err(LocalStorageMigrationError::TargetNotEmpty(display_path(
            &request.target_root,
        )));
    }

    let total_bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size));
    let available_bytes = available_space_for(&request.target_root)?;
    let fast_move = source_root.exists()
        && !target_nonempty
        && same_filesystem(&source_root, &request.target_root)?;
    let staging_bytes = pending
        .map(|pending| directory_size(pending.staging_root()))
        .transpose()?
        .unwrap_or(0);
    let required_bytes = if fast_move || resuming_completed_target {
        0
    } else {
        required_copy_space(total_bytes).saturating_sub(staging_bytes)
    };

    Ok(MigrationPlanInternal {
        request,
        source_root,
        entries,
        available_bytes,
        required_bytes,
        fast_move,
    })
}

fn resolve_request(
    manager: &LocalStorageManager,
    base_path: Option<String>,
) -> Result<MigrationRequest, LocalStorageMigrationError> {
    let location = match base_path {
        None => LocalStorageLocation::Default,
        Some(path) => {
            if !cfg!(target_os = "windows") && !cfg!(test) {
                return Err(LocalStorageMigrationError::UnsupportedPlatform);
            }
            let base_path = PathBuf::from(path);
            if !base_path.is_absolute() {
                return Err(LocalStorageMigrationError::RelativePath);
            }
            if !base_path.is_dir() {
                return Err(LocalStorageMigrationError::InvalidBasePath(display_path(
                    &base_path,
                )));
            }
            LocalStorageLocation::Custom {
                base_path: dunce::simplified(&base_path.canonicalize()?).to_path_buf(),
            }
        }
    };
    Ok(MigrationRequest {
        target_root: manager.root_for_location(&location),
        location,
    })
}

fn validate_path_relationship(
    source: &Path,
    target: &Path,
) -> Result<(), LocalStorageMigrationError> {
    if source == target {
        return Ok(());
    }
    if target.starts_with(source) || source.starts_with(target) {
        return Err(LocalStorageMigrationError::OverlappingPath);
    }
    Ok(())
}

fn directory_has_entries(path: &Path) -> Result<bool, std::io::Error> {
    match std::fs::read_dir(path) {
        Ok(mut entries) => Ok(entries.next().transpose()?.is_some()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn directory_size(path: &Path) -> Result<u64, std::io::Error> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
    }
    Ok(total)
}

fn staging_root(target_root: &Path) -> PathBuf {
    let name = target_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("los");
    target_root.with_file_name(format!("{name}.migrating"))
}

fn required_copy_space(total_bytes: u64) -> u64 {
    if total_bytes == 0 {
        return 0;
    }
    total_bytes.saturating_add(MINIMUM_FREE_SPACE_MARGIN.max(total_bytes / 20))
}

fn available_space_for(path: &Path) -> Result<u64, std::io::Error> {
    let ancestor = existing_ancestor(path).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "找不到目标目录所在磁盘")
    })?;
    fs4::available_space(ancestor)
}

fn existing_ancestor(path: &Path) -> Option<&Path> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.exists() {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

#[cfg(windows)]
fn same_filesystem(source: &Path, target: &Path) -> Result<bool, std::io::Error> {
    fn prefix(path: &Path) -> Option<String> {
        path.components().find_map(|component| match component {
            Component::Prefix(prefix) => {
                Some(prefix.as_os_str().to_string_lossy().to_ascii_lowercase())
            }
            _ => None,
        })
    }
    let source = source.canonicalize()?;
    let target = existing_ancestor(target)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "目标磁盘不存在"))?
        .canonicalize()?;
    Ok(prefix(&source).is_some() && prefix(&source) == prefix(&target))
}

#[cfg(unix)]
fn same_filesystem(source: &Path, target: &Path) -> Result<bool, std::io::Error> {
    use std::os::unix::fs::MetadataExt;
    let target = existing_ancestor(target)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "目标磁盘不存在"))?;
    Ok(std::fs::metadata(source)?.dev() == std::fs::metadata(target)?.dev())
}

#[cfg(not(any(windows, unix)))]
fn same_filesystem(_source: &Path, _target: &Path) -> Result<bool, std::io::Error> {
    Ok(false)
}

async fn copy_entries(
    source: &LocalObjectStore,
    target: &LocalObjectStore,
    entries: &[LocalObjectEntry],
    event: Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
) -> Result<(), LocalStorageMigrationError> {
    let total_bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size));
    let mut completed_bytes = 0u64;
    for (index, entry) in entries.iter().enumerate() {
        if object_matches(source, target, entry, &event).await? {
            completed_bytes = completed_bytes.saturating_add(entry.size);
            send_progress(
                &event,
                LocalStorageMigrationPhase::Copying,
                entry,
                index,
                entries.len(),
                entry.size,
                completed_bytes.saturating_sub(entry.size),
                total_bytes,
            );
            continue;
        }
        if target.get(&entry.key).await?.is_some() {
            target.delete(&entry.key).await?;
        }
        let mut stream = source.get_stream(&entry.key, None).await?;
        let handle = target.begin_chunked_save(&entry.key).await?;
        let mut current_bytes = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| crate::caches::CacheError::StreamError)?;
            if let Err(error) = handle.write_chunk(&chunk).await {
                handle.abort().await;
                return Err(error.into());
            }
            current_bytes = current_bytes.saturating_add(chunk.len() as u64);
            send_progress(
                &event,
                LocalStorageMigrationPhase::Copying,
                entry,
                index,
                entries.len(),
                current_bytes,
                completed_bytes,
                total_bytes,
            );
        }
        handle.finalize(&entry.etag).await?;
        completed_bytes = completed_bytes.saturating_add(entry.size);
    }
    Ok(())
}

async fn object_matches(
    source: &LocalObjectStore,
    target: &LocalObjectStore,
    entry: &LocalObjectEntry,
    event: &Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
) -> Result<bool, LocalStorageMigrationError> {
    if target.get(&entry.key).await?.as_deref() != Some(entry.etag.as_str())
        || target.get_size(&entry.key).await? != Some(entry.size)
    {
        return Ok(false);
    }
    let source_hash = hash_object(source, entry, None, 0, 0, 0, event).await?;
    let target_hash = hash_object(target, entry, None, 0, 0, 0, event).await?;
    Ok(source_hash == target_hash)
}

async fn verify_entries(
    source: &LocalObjectStore,
    target: &LocalObjectStore,
    entries: &[LocalObjectEntry],
    event: Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
) -> Result<(), LocalStorageMigrationError> {
    let total_bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size));
    let mut completed_bytes = 0u64;
    for (index, entry) in entries.iter().enumerate() {
        let target_etag = target.get(&entry.key).await?;
        let target_size = target.get_size(&entry.key).await?;
        if target_etag.as_deref() != Some(entry.etag.as_str()) || target_size != Some(entry.size) {
            return Err(LocalStorageMigrationError::VerificationFailed {
                key: entry.key.clone(),
            });
        }

        let source_hash = hash_object(source, entry, None, entries.len(), 0, 0, &event).await?;
        let target_hash = hash_object(
            target,
            entry,
            Some(index),
            entries.len(),
            completed_bytes,
            total_bytes,
            &event,
        )
        .await?;
        if source_hash != target_hash {
            return Err(LocalStorageMigrationError::VerificationFailed {
                key: entry.key.clone(),
            });
        }
        completed_bytes = completed_bytes.saturating_add(entry.size);
    }
    Ok(())
}

async fn hash_object(
    store: &LocalObjectStore,
    entry: &LocalObjectEntry,
    progress_index: Option<usize>,
    total_files: usize,
    completed_bytes: u64,
    total_bytes: u64,
    event: &Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
) -> Result<md5::Digest, LocalStorageMigrationError> {
    let mut stream = store.get_stream(&entry.key, None).await?;
    let mut context = md5::Context::new();
    let mut current_bytes = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| crate::caches::CacheError::StreamError)?;
        context.consume(&chunk);
        current_bytes = current_bytes.saturating_add(chunk.len() as u64);
        if let Some(index) = progress_index {
            send_progress(
                event,
                LocalStorageMigrationPhase::Verifying,
                entry,
                index,
                total_files,
                current_bytes,
                completed_bytes,
                total_bytes,
            );
        }
    }
    Ok(context.finalize())
}

#[allow(clippy::too_many_arguments)]
fn send_progress(
    event: &Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
    phase: LocalStorageMigrationPhase,
    entry: &LocalObjectEntry,
    index: usize,
    total_files: usize,
    current_file_bytes: u64,
    completed_bytes: u64,
    total_bytes: u64,
) {
    let _ = event.send(LocalStorageMigrationEvent::Progress {
        phase,
        current_file: entry.key.clone(),
        current_file_index: index as u32 + 1,
        total_files: total_files as u32,
        current_file_bytes: current_file_bytes.min(entry.size),
        current_file_size: entry.size,
        processed_bytes: completed_bytes
            .saturating_add(current_file_bytes)
            .min(total_bytes),
        total_bytes,
    });
}

fn send_completed(
    event: &Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
    plan: &MigrationPlanInternal,
    cleanup_warning: Option<String>,
) {
    let _ = event.send(LocalStorageMigrationEvent::Completed {
        target_path: display_path(&plan.request.target_root),
        migrated_files: plan.entries.len() as u32,
        migrated_bytes: plan.total_bytes(),
        cleanup_warning,
    });
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn manager(temp_dir: &tempfile::TempDir) -> LocalStorageManager {
        LocalStorageManager::new(
            crate::app_config::AppConfigStore::in_memory(crate::app_config::AppConfig::default()),
            temp_dir.path().join("local-data"),
            temp_dir.path().join("cache"),
        )
    }

    #[tokio::test]
    async fn custom_plan_appends_los_and_counts_objects() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let target_base = temp_dir.path().join("target");
        std::fs::create_dir_all(&target_base).unwrap();
        let source = LocalObjectStore::new(source_dir);
        source
            .save_bytes("1/manifest.enc", b"manifest")
            .await
            .unwrap();
        source.save_bytes("1/att-1", b"attachment").await.unwrap();

        let request =
            resolve_request(&manager(&temp_dir), Some(display_path(&target_base))).unwrap();
        let plan = build_plan(&source, request).await.unwrap().public();

        assert_eq!(PathBuf::from(plan.target_path), target_base.join("los"));
        assert_eq!(plan.total_files, 2);
        assert_eq!(plan.total_bytes, 18);
    }

    #[tokio::test]
    async fn streaming_copy_preserves_nested_objects_and_etags() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source = LocalObjectStore::new(temp_dir.path().join("source"));
        let target = LocalObjectStore::new(temp_dir.path().join("target"));
        source
            .save_bytes("123/manifest.enc", b"manifest-data")
            .await
            .unwrap();
        source
            .save_bytes("123/att-1", &vec![7; 1024 * 1024])
            .await
            .unwrap();
        let entries = source.get_all_entries().await.unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel();
        let sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(sender);

        copy_entries(&source, &target, &entries, sender.clone())
            .await
            .unwrap();
        verify_entries(&source, &target, &entries, sender)
            .await
            .unwrap();

        for entry in entries {
            assert_eq!(
                source.get(&entry.key).await.unwrap(),
                target.get(&entry.key).await.unwrap()
            );
            assert_eq!(
                source.get_data(&entry.key).await.unwrap(),
                target.get_data(&entry.key).await.unwrap()
            );
        }
    }

    #[tokio::test]
    async fn verification_rejects_same_size_corruption() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source = LocalObjectStore::new(temp_dir.path().join("source"));
        let target = LocalObjectStore::new(temp_dir.path().join("target"));
        source.save_bytes("1/att", b"source").await.unwrap();
        let entries = source.get_all_entries().await.unwrap();
        target
            .save_stream_with_etag(
                "1/att",
                &entries[0].etag,
                Box::pin(futures_util::stream::once(async {
                    Ok(bytes::Bytes::from_static(b"target"))
                })),
            )
            .await
            .unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel();
        let sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(sender);

        assert!(matches!(
            verify_entries(&source, &target, &entries, sender).await,
            Err(LocalStorageMigrationError::VerificationFailed { .. })
        ));
    }

    #[test]
    fn rejects_relative_custom_path_and_overlapping_roots() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            resolve_request(&manager(&temp_dir), Some("relative".into())),
            Err(LocalStorageMigrationError::RelativePath)
        ));
        let source = temp_dir.path().join("source");
        assert!(matches!(
            validate_path_relationship(&source, &source.join("los")),
            Err(LocalStorageMigrationError::OverlappingPath)
        ));
    }

    #[test]
    fn required_space_includes_safety_margin() {
        assert_eq!(required_copy_space(0), 0);
        assert_eq!(required_copy_space(100), 100 + MINIMUM_FREE_SPACE_MARGIN);
        let large = 100 * 1024 * 1024 * 1024u64;
        assert_eq!(required_copy_space(large), large + large / 20);
    }

    #[test]
    fn directory_size_counts_existing_staging_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested = temp_dir.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(temp_dir.path().join("object.data"), vec![1; 512]).unwrap();
        std::fs::write(nested.join("object.md5"), vec![2; 32]).unwrap();

        assert_eq!(directory_size(temp_dir.path()).unwrap(), 544);
        assert_eq!(directory_size(&temp_dir.path().join("missing")).unwrap(), 0);
    }

    #[tokio::test]
    async fn legacy_directory_is_moved_to_default_los_and_committed() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = manager(&temp_dir);
        let source_root = temp_dir.path().join("cache").join("lfc");
        let target_root = temp_dir.path().join("local-data").join("los");
        let source = LocalObjectStore::new(source_root.clone());
        source
            .save_bytes("123/manifest.enc", b"legacy-manifest")
            .await
            .unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel();
        let sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(sender);

        execute_migration(source, manager.clone(), sender, None, true)
            .await
            .unwrap();

        assert!(!source_root.exists());
        assert_eq!(
            LocalObjectStore::new(target_root)
                .get_data("123/manifest.enc")
                .await
                .unwrap(),
            b"legacy-manifest"
        );
        assert!(manager.pending_migration().is_none());
        assert_eq!(manager.configured_location(), LocalStorageLocation::Default);
    }

    #[tokio::test]
    async fn forced_copy_to_custom_location_switches_only_after_verification() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = manager(&temp_dir);
        let source_root = temp_dir.path().join("source");
        let target_base = temp_dir.path().join("target-base");
        std::fs::create_dir_all(&target_base).unwrap();
        let source = LocalObjectStore::new(source_root.clone());
        source
            .save_bytes("123/manifest.enc", b"manifest")
            .await
            .unwrap();
        source
            .save_bytes("123/att-1", &vec![9; 2 * 1024 * 1024])
            .await
            .unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel();
        let sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(sender);

        execute_migration(
            source,
            manager.clone(),
            sender,
            Some(display_path(&target_base)),
            false,
        )
        .await
        .unwrap();

        let target = LocalObjectStore::new(target_base.join("los"));
        assert!(!source_root.exists());
        assert_eq!(
            target.get_data("123/manifest.enc").await.unwrap(),
            b"manifest"
        );
        assert_eq!(
            target.get_size("123/att-1").await.unwrap(),
            Some(2 * 1024 * 1024)
        );
        assert_eq!(
            manager.configured_location(),
            LocalStorageLocation::Custom {
                base_path: dunce::simplified(&target_base.canonicalize().unwrap()).to_path_buf(),
            }
        );
        assert!(manager.pending_migration().is_none());
    }

    #[tokio::test]
    async fn pending_migration_with_completed_target_is_verified_and_resumed() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = manager(&temp_dir);
        let source_root = temp_dir.path().join("source");
        let target_base = temp_dir.path().join("target-base");
        let target_root = target_base.join("los");
        std::fs::create_dir_all(&target_base).unwrap();
        let source = LocalObjectStore::new(source_root.clone());
        source.save_bytes("123/att", b"resume-data").await.unwrap();
        let target = LocalObjectStore::new(target_root.clone());
        let entries = source.get_all_entries().await.unwrap();
        let (copy_sender, _copy_receiver) = mpsc::unbounded_channel();
        let copy_sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(copy_sender);
        copy_entries(&source, &target, &entries, copy_sender)
            .await
            .unwrap();
        let location = LocalStorageLocation::Custom {
            base_path: target_base.canonicalize().unwrap(),
        };
        manager
            .config()
            .begin_local_storage_migration(PendingLocalStorageMigration::new(
                source_root.clone(),
                target_root,
                target_base.join("los.migrating"),
                location.clone(),
            ))
            .unwrap();
        let pending = manager.pending_migration().unwrap();
        let resumed_plan = build_plan_for_resume(
            &source,
            MigrationRequest {
                location: location.clone(),
                target_root: pending.target_root().to_path_buf(),
            },
            Some(&pending),
        )
        .await
        .unwrap();
        assert_eq!(resumed_plan.required_bytes, 0);
        let (sender, _receiver) = mpsc::unbounded_channel();
        let sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(sender);

        execute_migration(source, manager.clone(), sender, None, false)
            .await
            .unwrap();

        assert!(!source_root.exists());
        assert_eq!(manager.configured_location(), location);
        assert!(manager.pending_migration().is_none());
    }
}
