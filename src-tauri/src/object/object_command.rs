use crate::diaries::diary_sync::{sync_cloud_to_local, sync_local_to_cloud, SyncProgressEvent};
use crate::error::AppError;
use crate::state::AppState;
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
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_init_oss_client(
    state: State<'_, AppState>,
    akid: String,
    aks: String,
    bucket: String,
    endpoint: String,
) -> Result<(), AppError> {
    log::info!("[oss cmd] akid(len={}): {:?}", akid.len(), akid);
    log::info!("[oss cmd] bucket(len={}): {:?}", bucket.len(), bucket);
    log::info!("[oss cmd] endpoint(len={}): {:?}", endpoint.len(), endpoint);
    Ok(state.oss_client().initialize(endpoint, akid, aks, bucket)?)
}

/// 启用远程存储：初始化 OSS 客户端 → 同步本地数据到云端 → 设置 remote_enabled
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
    log::info!("[remote] enabling remote storage...");

    // 1. 初始化 OSS 客户端
    state.oss_client().initialize(endpoint, akid, aks, bucket)?;

    // 2. 同步本地数据到云端
    if let Err(error) =
        sync_local_to_cloud(&state.local_file_cache(), &state.oss_client(), &event).await
    {
        let message = error.to_string();
        let _ = event.send(SyncProgressEvent::Error(message.clone()));
        return Err(AppError {
            error_type: "sync".into(),
            message,
        });
    }

    // 3. 设置远程存储启用
    state.set_remote_enabled(true);
    log::info!("[remote] remote storage enabled successfully");
    Ok(())
}

/// 禁用远程存储：同步云端数据到本地 → 设置 remote_enabled = false → 重置 OSS 客户端
#[tauri::command]
#[specta::specta]
pub async fn cmd_disable_remote_storage(
    state: State<'_, AppState>,
    event: Channel<SyncProgressEvent>,
) -> Result<(), AppError> {
    log::info!("[remote] disabling remote storage...");

    // 1. 同步云端数据到本地
    if let Err(error) =
        sync_cloud_to_local(&state.local_file_cache(), &state.oss_client(), &event).await
    {
        let message = error.to_string();
        let _ = event.send(SyncProgressEvent::Error(message.clone()));
        return Err(AppError {
            error_type: "sync".into(),
            message,
        });
    }

    // 2. 设置远程存储禁用
    state.set_remote_enabled(false);

    // 3. 重置 OSS 客户端
    state.oss_client().reset();
    log::info!("[remote] remote storage disabled successfully");
    Ok(())
}

/// 获取当前存储模式
#[tauri::command]
#[specta::specta]
pub fn cmd_get_storage_mode(state: State<'_, AppState>) -> bool {
    state.is_remote_enabled()
}

/// 设置远程存储启用状态（解锁时从前端配置恢复）
#[tauri::command]
#[specta::specta]
pub fn cmd_set_remote_enabled(state: State<'_, AppState>, enabled: bool) {
    state.set_remote_enabled(enabled);
    log::info!("[remote] remote_enabled set to {}", enabled);
}
