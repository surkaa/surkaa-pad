use crate::app_config::{LocalStorageLocation, PendingLocalStorageMigration};
use crate::caches::{LocalObjectEntry, LocalObjectStore};
use crate::error::AppError;
use crate::local_storage::{available_space_for, LocalStorageManager};
use crate::state::AppState;
use crate::utils::message_sender::MessageSender;
use serde::Serialize;
use specta::Type;
use std::path::Path;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_log::log;
use tauri_plugin_opener::OpenerExt;
use thiserror::Error;

mod plan;
#[cfg(test)]
mod tests;
mod transfer;

use plan::{
    build_plan, build_plan_for_resume, directory_has_entries, resolve_request, staging_root,
    MigrationPlanInternal, MigrationRequest,
};
use transfer::{copy_entries, verify_entries};

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

/// 使用系统文件管理器打开当前实际使用的本地对象存储目录。
/// 路径只从后端状态读取，不接受前端传入路径，避免为 opener 放开任意目录权限。
#[tauri::command]
#[specta::specta]
pub async fn cmd_open_local_storage(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    let path = display_path(state.local_object_store().root());
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|error| AppError {
            error_type: "open_local_storage".into(),
            message: error.to_string(),
        })
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

/// 获取当前本地对象存储位置和数据规模。
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_local_storage_info(
    state: State<'_, AppState>,
) -> Result<LocalStorageInfo, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
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
    let _storage_guard = state.lock_storage_operation().await;
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
