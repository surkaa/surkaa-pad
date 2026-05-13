use crate::error::AppError;
use crate::state::AppState;
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
    Ok(state
        .oss_client()
        .initialize(endpoint, akid, aks, bucket)?)
}
