use crate::caches::AttachmentCacheStats;
use crate::diaries::diary_sync::{
    inspect_cloud_to_local, sync_cloud_to_local, sync_local_to_cloud, SyncDirection, SyncPhase,
    SyncProgressEvent,
};
use crate::error::AppError;
use crate::local_storage::{available_space_for, required_space_with_margin};
use crate::state::AppState;
use crate::utils::message_sender::MessageSender;
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::State;
use tauri_plugin_log::log;

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisableRemoteStoragePlan {
    #[specta(rename = "localStoragePath")]
    pub local_storage_path: String,
    #[specta(rename = "remoteFiles")]
    pub remote_files: u32,
    #[specta(rename = "remoteBytes", type = f64)]
    pub remote_bytes: u64,
    #[specta(rename = "skippedFiles")]
    pub skipped_files: u32,
    #[specta(rename = "skippedBytes", type = f64)]
    pub skipped_bytes: u64,
    #[specta(rename = "downloadFiles")]
    pub download_files: u32,
    #[specta(rename = "downloadBytes", type = f64)]
    pub download_bytes: u64,
    #[specta(rename = "availableBytes", type = f64)]
    pub available_bytes: u64,
    #[specta(rename = "hasSufficientSpace")]
    pub has_sufficient_space: bool,
}

const MIN_ATTACHMENT_CACHE_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ATTACHMENT_CACHE_LIMIT_BYTES: u64 = 100 * 1024 * 1024 * 1024;
const MIN_ATTACHMENT_CACHE_FILE_SIZE_BYTES: u64 = 1024 * 1024;
const MAX_ATTACHMENT_CACHE_FILE_SIZE_BYTES: u64 = 100 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCacheInfo {
    pub cached_files: u32,
    #[specta(type = f64)]
    pub cached_bytes: u64,
    #[specta(type = f64)]
    pub limit_bytes: u64,
    #[specta(type = f64)]
    pub max_file_size_bytes: u64,
}

impl From<AttachmentCacheStats> for AttachmentCacheInfo {
    fn from(value: AttachmentCacheStats) -> Self {
        Self {
            cached_files: value.cached_files,
            cached_bytes: value.cached_bytes,
            limit_bytes: value.limit_bytes,
            max_file_size_bytes: value.max_file_size_bytes,
        }
    }
}

/// 初始化 OSS 客户端
/// # Arguments
/// * `akid` - 访问密钥 ID
/// * `aks` - 访问密钥 Secret
/// * `bucket` - 存储桶名称
/// * `endpoint` - OSS 端点
/// # Returns
/// * `Result<(), AppError>` - 成功时完成 OSS 客户端初始化，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_init_oss_client(
    state: State<'_, AppState>,
    akid: String,
    aks: String,
    bucket: String,
    endpoint: String,
) -> Result<(), AppError> {
    let _storage_mode_guard = state
        .try_lock_storage_mode_change()
        .ok_or_else(|| AppError {
            error_type: "storage_busy".into(),
            message: "有存储操作正在进行，请等待完成后再初始化云存储".into(),
        })?;
    log::info!("[oss cmd] bucket(len={}): {:?}", bucket.len(), bucket);
    log::info!("[oss cmd] endpoint(len={}): {:?}", endpoint.len(), endpoint);
    Ok(state.oss_client().initialize(endpoint, akid, aks, bucket)?)
}

/// 启用远程存储：初始化 OSS 客户端 → 同步本地数据到云端 → 设置 remote_enabled
/// # Arguments
/// * `event` - 接收同步进度与错误事件的通道
/// * `akid` - 访问密钥 ID
/// * `aks` - 访问密钥 Secret
/// * `bucket` - 存储桶名称
/// * `endpoint` - OSS 端点
/// # Returns
/// * `Result<(), AppError>` - 成功时已完成数据上传并切换为远程存储
#[tauri::command]
#[specta::specta]
pub async fn cmd_enable_remote_storage(
    state: State<'_, AppState>,
    event: Channel<SyncProgressEvent>,
    akid: String,
    aks: String,
    bucket: String,
    endpoint: String,
) -> Result<(), AppError> {
    let _storage_mode_guard = state
        .try_lock_storage_mode_change()
        .ok_or_else(|| AppError {
            error_type: "storage_busy".into(),
            message: "有存储操作正在进行，请等待完成后再切换云存储".into(),
        })?;
    log::info!("[remote] enabling remote storage...");
    let event: Arc<dyn MessageSender<SyncProgressEvent>> = Arc::new(event);
    let direction = SyncDirection::Upload;
    let _ = event.send(SyncProgressEvent::Preparing { direction });

    // 1. 初始化 OSS 客户端
    if let Err(error) = state.oss_client().initialize(endpoint, akid, aks, bucket) {
        let message = error.to_string();
        log::error!("[remote] OSS initialization failed: {message}");
        let _ = event.send(SyncProgressEvent::Error {
            direction,
            phase: SyncPhase::Preparing,
            current_file: None,
            message: message.clone(),
        });
        return Err(AppError {
            error_type: "sync".into(),
            message,
        });
    }

    if let Err(error) = state
        .vault_bootstrap_repository()
        .ensure_remote_for_active_key()
        .await
    {
        let message = error.to_string();
        log::error!("[remote] Vault bootstrap validation failed: {message}");
        let _ = event.send(SyncProgressEvent::Error {
            direction,
            phase: SyncPhase::Preparing,
            current_file: None,
            message: message.clone(),
        });
        state.oss_client().reset();
        return Err(AppError {
            error_type: "vault_bootstrap".into(),
            message,
        });
    }

    // 2. 同步本地数据到云端
    let summary = match sync_local_to_cloud(
        &state.local_object_store(),
        &state.oss_client(),
        event.clone(),
    )
    .await
    {
        Ok(summary) => summary,
        Err(error) => {
            let message = error.message.clone();
            log::error!(
                "[remote] upload sync failed: phase={:?}, current_file={:?}, error={}",
                error.phase,
                error.current_file,
                message
            );
            let _ = event.send(error.into_event(direction));
            state.oss_client().reset();
            return Err(AppError {
                error_type: "sync".into(),
                message,
            });
        }
    };

    // 3. 持久化配置后再启用运行时远程存储
    state.persist_remote_enabled(true)?;
    state.set_remote_enabled(true);
    if let Err(error) = state.attachment_cache().activate().await {
        log::warn!("[remote] attachment cache activation failed: {error}");
    }
    let _ = event.send(SyncProgressEvent::Completed {
        direction,
        transferred_files: summary.transferred_files,
        skipped_files: summary.skipped_files,
        transferred_bytes: summary.transferred_bytes,
    });
    log::info!("[remote] remote storage enabled successfully");
    Ok(())
}

/// 只读取云端与本地对象元数据，规划关闭远程存储所需的下载和磁盘空间。
/// 不读取对象正文，也不会修改当前存储模式。
/// # Returns
/// * `Result<DisableRemoteStoragePlan, AppError>` - 待下载数据、跳过数据、实际本地目录和容量信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_plan_disable_remote_storage(
    state: State<'_, AppState>,
) -> Result<DisableRemoteStoragePlan, AppError> {
    if !state.is_remote_enabled() {
        return Err(AppError {
            error_type: "storage_mode".into(),
            message: "云存储当前未启用".into(),
        });
    }
    let _storage_mode_guard = state
        .try_lock_storage_mode_change()
        .ok_or_else(|| AppError {
            error_type: "storage_busy".into(),
            message: "有存储操作正在进行，请等待完成后再检查本地空间".into(),
        })?;
    let los = state.local_object_store();
    let stats = inspect_cloud_to_local(&los, &state.oss_client())
        .await
        .map_err(|error| AppError {
            error_type: "sync_plan".into(),
            message: error.message,
        })?;
    let available_bytes = available_space_for(los.root()).map_err(|error| AppError {
        error_type: "sync_plan".into(),
        message: format!("无法读取本地存储可用空间: {error}"),
    })?;
    // 安全余量只参与是否允许下载的内部判定；对用户展示的实际需求始终是 download_bytes。
    let protected_required_bytes = required_space_with_margin(stats.download_bytes);
    let plan = DisableRemoteStoragePlan {
        local_storage_path: los.root().to_string_lossy().into_owned(),
        remote_files: stats.remote_files,
        remote_bytes: stats.remote_bytes,
        skipped_files: stats.skipped_files,
        skipped_bytes: stats.skipped_bytes,
        download_files: stats.download_files,
        download_bytes: stats.download_bytes,
        available_bytes,
        has_sufficient_space: available_bytes >= protected_required_bytes,
    };
    log::info!(
        "[remote] disable plan ready: remote_files={}, remote_bytes={}, download_files={}, download_bytes={}, skipped_files={}, available_bytes={}, protected_required_bytes={}, sufficient={}",
        plan.remote_files,
        plan.remote_bytes,
        plan.download_files,
        plan.download_bytes,
        plan.skipped_files,
        plan.available_bytes,
        protected_required_bytes,
        plan.has_sufficient_space
    );
    Ok(plan)
}

/// 禁用远程存储：重新规划并校验空间 → 同步云端数据到本地 → 设置 remote_enabled = false → 重置 OSS 客户端
/// # Arguments
/// * `event` - 接收同步进度与错误事件的通道
/// # Returns
/// * `Result<(), AppError>` - 成功时已完成数据下载并切换为本地存储
#[tauri::command]
#[specta::specta]
pub async fn cmd_disable_remote_storage(
    state: State<'_, AppState>,
    event: Channel<SyncProgressEvent>,
) -> Result<(), AppError> {
    log::info!("[remote] disabling remote storage...");
    let event: Arc<dyn MessageSender<SyncProgressEvent>> = Arc::new(event);
    let direction = SyncDirection::Download;
    let _ = event.send(SyncProgressEvent::Preparing { direction });

    // 已处于本地模式时按幂等成功处理，避免状态恢复期间重复关闭访问未初始化的 OSS。
    if !state.is_remote_enabled() {
        state.persist_remote_enabled(false)?;
        let _ = event.send(SyncProgressEvent::Completed {
            direction,
            transferred_files: 0,
            skipped_files: 0,
            transferred_bytes: 0,
        });
        log::info!("[remote] remote storage already disabled");
        return Ok(());
    }

    let _storage_mode_guard = state
        .try_lock_storage_mode_change()
        .ok_or_else(|| AppError {
            error_type: "storage_busy".into(),
            message: "有存储操作正在进行，请等待完成后再切换云存储".into(),
        })?;

    // 1. 同步云端数据到本地
    let summary = match sync_cloud_to_local(
        &state.local_object_store(),
        &state.oss_client(),
        event.clone(),
    )
    .await
    {
        Ok(summary) => summary,
        Err(error) => {
            let message = error.message.clone();
            let _ = event.send(error.into_event(direction));
            return Err(AppError {
                error_type: "sync".into(),
                message,
            });
        }
    };

    // 2. 先持久化配置，再设置运行时远程存储禁用
    state.persist_remote_enabled(false)?;
    state.set_remote_enabled(false);
    if let Err(error) = state.attachment_cache().deactivate().await {
        log::warn!("[remote] failed to remove attachment cache index: {error}");
    }

    // 3. 重置 OSS 客户端
    state.oss_client().reset();
    let _ = event.send(SyncProgressEvent::Completed {
        direction,
        transferred_files: summary.transferred_files,
        skipped_files: summary.skipped_files,
        transferred_bytes: summary.transferred_bytes,
    });
    log::info!("[remote] remote storage disabled successfully");
    Ok(())
}

/// 获取持久化的存储模式
/// # Returns
/// * `bool` - `true` 表示配置为远程存储，`false` 表示本地存储
#[tauri::command]
#[specta::specta]
pub fn cmd_get_storage_mode(state: State<'_, AppState>) -> bool {
    state
        .configured_remote_enabled()
        .unwrap_or_else(|| state.is_remote_enabled())
}

/// 获取云同步模式下的本地附件缓存用量和容量上限。
/// # Returns
/// * `Result<AttachmentCacheInfo, AppError>` - 已缓存附件数量、总大小和容量上限
#[tauri::command]
#[specta::specta]
pub async fn cmd_get_attachment_cache_info(
    state: State<'_, AppState>,
) -> Result<AttachmentCacheInfo, AppError> {
    if !state.is_remote_enabled() {
        return Err(AppError {
            error_type: "storage_mode".into(),
            message: "只有启用云同步后才能管理本地附件缓存".into(),
        });
    }
    Ok(state.attachment_cache().enforce_limit().await?.into())
}

/// 修改云同步模式下的本地附件缓存容量上限，并立即按 LRU 淘汰到新上限。
/// # Arguments
/// * `limit_bytes` - 缓存容量上限，允许范围为 1–100 GiB
/// # Returns
/// * `Result<AttachmentCacheInfo, AppError>` - 应用新上限后的缓存统计
#[tauri::command]
#[specta::specta]
pub async fn cmd_set_attachment_cache_limit(
    state: State<'_, AppState>,
    limit_bytes: f64,
) -> Result<AttachmentCacheInfo, AppError> {
    if !state.is_remote_enabled() {
        return Err(AppError {
            error_type: "storage_mode".into(),
            message: "只有启用云同步后才能修改本地附件缓存上限".into(),
        });
    }
    if !limit_bytes.is_finite()
        || limit_bytes.fract() != 0.0
        || limit_bytes < MIN_ATTACHMENT_CACHE_LIMIT_BYTES as f64
        || limit_bytes > MAX_ATTACHMENT_CACHE_LIMIT_BYTES as f64
    {
        return Err(AppError {
            error_type: "cache_limit".into(),
            message: "本地附件缓存上限必须在 1–100 GB 之间".into(),
        });
    }
    let limit_bytes = limit_bytes as u64;

    let previous_limit = state.attachment_cache_limit_bytes();
    state.persist_attachment_cache_limit_bytes(limit_bytes)?;
    match state.attachment_cache().enforce_limit().await {
        Ok(stats) => Ok(stats.into()),
        Err(error) => {
            if let Err(rollback_error) = state.persist_attachment_cache_limit_bytes(previous_limit)
            {
                log::error!(
                    "[cache] failed to restore cache limit after enforcement error: {rollback_error}"
                );
            }
            Err(error.into())
        }
    }
}

/// 修改云同步模式下单个附件允许写入本地缓存的大小上限。
/// 超过上限的附件仍会正常上传和读取，但不会保留本地副本。
/// # Arguments
/// * `limit_bytes` - 单个附件缓存上限，允许范围为 1 MiB–100 GiB
/// # Returns
/// * `Result<AttachmentCacheInfo, AppError>` - 应用新上限并清理超限附件后的缓存统计
#[tauri::command]
#[specta::specta]
pub async fn cmd_set_attachment_cache_max_file_size(
    state: State<'_, AppState>,
    limit_bytes: f64,
) -> Result<AttachmentCacheInfo, AppError> {
    if !state.is_remote_enabled() {
        return Err(AppError {
            error_type: "storage_mode".into(),
            message: "只有启用云同步后才能修改单个附件缓存上限".into(),
        });
    }
    if !limit_bytes.is_finite()
        || limit_bytes.fract() != 0.0
        || limit_bytes < MIN_ATTACHMENT_CACHE_FILE_SIZE_BYTES as f64
        || limit_bytes > MAX_ATTACHMENT_CACHE_FILE_SIZE_BYTES as f64
    {
        return Err(AppError {
            error_type: "cache_limit".into(),
            message: "单个附件缓存上限必须在 1 MB–100 GB 之间".into(),
        });
    }
    let limit_bytes = limit_bytes as u64;
    let previous_limit = state.attachment_cache_max_file_size_bytes();
    state.persist_attachment_cache_max_file_size_bytes(limit_bytes)?;
    match state.attachment_cache().enforce_limit().await {
        Ok(stats) => Ok(stats.into()),
        Err(error) => {
            if let Err(rollback_error) =
                state.persist_attachment_cache_max_file_size_bytes(previous_limit)
            {
                log::error!("[cache] failed to restore per-file cache limit: {rollback_error}");
            }
            Err(error.into())
        }
    }
}

/// 将旧版前端保存的远程存储状态迁移到 Rust 配置。
/// # Arguments
/// * `legacy_enabled` - 旧版前端配置中的远程存储状态
/// # Returns
/// * `Result<bool, AppError>` - Rust 配置中最终采用的远程存储状态
#[tauri::command]
#[specta::specta]
pub fn cmd_migrate_legacy_remote_enabled(
    state: State<'_, AppState>,
    legacy_enabled: bool,
) -> Result<bool, AppError> {
    Ok(state.initialize_configured_remote_enabled(legacy_enabled)?)
}

/// OSS 客户端初始化后，根据 Rust 配置恢复当前进程的远程存储状态。
/// # Returns
/// * `Result<bool, AppError>` - 当前进程最终启用的远程存储状态
#[tauri::command]
#[specta::specta]
pub async fn cmd_restore_remote_storage(state: State<'_, AppState>) -> Result<bool, AppError> {
    let _storage_mode_guard = state
        .try_lock_storage_mode_change()
        .ok_or_else(|| AppError {
            error_type: "storage_busy".into(),
            message: "有存储操作正在进行，请等待完成后再恢复存储模式".into(),
        })?;
    let enabled = state.configured_remote_enabled().unwrap_or(false);
    if enabled && !state.oss_client().is_initialized() {
        return Err(AppError {
            error_type: "oss_not_initialized".into(),
            message: "远程存储已配置，但 OSS 客户端尚未初始化".into(),
        });
    }
    if enabled {
        state
            .vault_bootstrap_repository()
            .ensure_remote_for_active_key()
            .await?;
    }
    state.set_remote_enabled(enabled);
    if enabled {
        if let Err(error) = state.attachment_cache().activate().await {
            log::warn!("[remote] attachment cache restore failed: {error}");
        }
    } else if let Err(error) = state.attachment_cache().deactivate().await {
        log::warn!("[remote] failed to remove inactive attachment cache index: {error}");
    }
    log::info!("[remote] runtime storage mode restored: remote={enabled}");
    Ok(enabled)
}
