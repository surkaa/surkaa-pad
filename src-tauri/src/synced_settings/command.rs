use super::types::{SyncedSettingsData, SyncedSettingsDocument};
use crate::error::AppError;
use crate::state::AppState;
use chrono::Utc;
use tauri::State;

fn ensure_remote_enabled(state: &AppState) -> Result<(), AppError> {
    if state.is_remote_enabled() {
        return Ok(());
    }
    Err(AppError {
        error_type: "remote_disabled".into(),
        message: "只有启用云同步后才能同步应用设置".into(),
    })
}

/// 读取云端加密的可同步应用设置。
/// # Returns
/// * `Result<Option<SyncedSettingsDocument>, AppError>` - 尚未创建设置对象时返回 `None`
#[tauri::command]
#[specta::specta]
pub async fn cmd_load_synced_settings(
    state: State<'_, AppState>,
) -> Result<Option<SyncedSettingsDocument>, AppError> {
    load_synced_settings(state.inner()).await
}

/// 验证并加密保存可跨设备同步的应用设置。
/// # Arguments
/// * `settings` - 不含设备凭据、缓存限制等本机配置的设置数据
/// # Returns
/// * `Result<SyncedSettingsDocument, AppError>` - 包含后端生成版本号与更新时间的完整文档
#[tauri::command]
#[specta::specta]
pub async fn cmd_save_synced_settings(
    state: State<'_, AppState>,
    settings: SyncedSettingsData,
) -> Result<SyncedSettingsDocument, AppError> {
    save_synced_settings(state.inner(), settings).await
}

async fn load_synced_settings(
    state: &AppState,
) -> Result<Option<SyncedSettingsDocument>, AppError> {
    ensure_remote_enabled(state)?;
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state.synced_settings_repository().load().await?)
}

async fn save_synced_settings(
    state: &AppState,
    settings: SyncedSettingsData,
) -> Result<SyncedSettingsDocument, AppError> {
    ensure_remote_enabled(state)?;
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state
        .synced_settings_repository()
        .save(settings, Utc::now().timestamp_millis())
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::LocalObjectStore;
    use crate::cryptos::Crypto;
    use crate::object::OssClient;

    #[tokio::test]
    async fn rejects_commands_while_remote_storage_is_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let state = AppState::from_parts(
            Crypto::new(),
            OssClient::new(),
            LocalObjectStore::new(temp.path().to_path_buf()),
        );

        let error = load_synced_settings(&state).await.unwrap_err();
        assert_eq!(error.error_type, "remote_disabled");
    }
}
