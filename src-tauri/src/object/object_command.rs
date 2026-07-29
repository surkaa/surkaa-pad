use crate::diaries::diary_sync::{
    sync_cloud_to_local, sync_local_to_cloud, SyncDirection, SyncPhase, SyncProgressEvent,
};
use crate::error::AppError;
use crate::state::AppState;
use crate::utils::message_sender::MessageSender;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::State;
use tauri_plugin_log::log;

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
    let _ = event.send(SyncProgressEvent::Completed {
        direction,
        transferred_files: summary.transferred_files,
        skipped_files: summary.skipped_files,
        transferred_bytes: summary.transferred_bytes,
    });
    log::info!("[remote] remote storage enabled successfully");
    Ok(())
}

/// 禁用远程存储：同步云端数据到本地 → 设置 remote_enabled = false → 重置 OSS 客户端
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
pub fn cmd_restore_remote_storage(state: State<'_, AppState>) -> Result<bool, AppError> {
    let enabled = state.configured_remote_enabled().unwrap_or(false);
    if enabled && !state.oss_client().is_initialized() {
        return Err(AppError {
            error_type: "oss_not_initialized".into(),
            message: "远程存储已配置，但 OSS 客户端尚未初始化".into(),
        });
    }
    state.set_remote_enabled(enabled);
    log::info!("[remote] runtime storage mode restored: remote={enabled}");
    Ok(enabled)
}
