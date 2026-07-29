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

    // 3. 设置远程存储启用
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

    // 2. 设置远程存储禁用
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

/// 获取当前存储模式
/// # Returns
/// * `bool` - `true` 表示已启用远程存储，`false` 表示本地存储
#[tauri::command]
#[specta::specta]
pub fn cmd_get_storage_mode(state: State<'_, AppState>) -> bool {
    state.is_remote_enabled()
}

/// 设置远程存储启用状态（解锁时从前端配置恢复）
/// # Arguments
/// * `enabled` - 是否启用远程存储
/// # Returns
/// * `()` - 无返回数据
#[tauri::command]
#[specta::specta]
pub fn cmd_set_remote_enabled(state: State<'_, AppState>, enabled: bool) {
    state.set_remote_enabled(enabled);
    log::info!("[remote] remote_enabled set to {}", enabled);
}
