use super::VaultBootstrap;
use crate::error::AppError;
use crate::state::AppState;
use tauri::State;
use tauri_plugin_log::log;

/// 在旧版密码校验成功后，为当前 Vault 补建并持久化密钥派生引导配置。
/// # Returns
/// * `Result<VaultBootstrap, AppError>` - 当前 Vault 的完整引导配置
#[tauri::command]
#[specta::specta]
pub fn cmd_commit_vault_bootstrap(state: State<'_, AppState>) -> Result<VaultBootstrap, AppError> {
    Ok(state.vault_bootstrap_repository().commit_active()?)
}

/// 获取当前 Vault 的密钥派生引导配置。
/// # Returns
/// * `Result<VaultBootstrap, AppError>` - 当前配置；尚未完成旧版迁移时返回错误
#[tauri::command]
#[specta::specta]
pub fn cmd_get_vault_bootstrap(state: State<'_, AppState>) -> Result<VaultBootstrap, AppError> {
    Ok(state.vault_bootstrap_repository().get_required()?)
}

/// 判断当前设备是否已经保存 Vault 密钥派生配置。
/// # Returns
/// * `bool` - 已保存时为 `true`
#[tauri::command]
#[specta::specta]
pub fn cmd_has_vault_bootstrap(state: State<'_, AppState>) -> bool {
    state.vault_bootstrap().is_some()
}

/// 初始化一个确认没有历史对象的新 Vault，并使用随机盐派生密钥。
/// # Arguments
/// * `master_password` - 新 Vault 的主密码
/// * `memory_cost_kib` - 用户选择的 Argon2id 内存成本（KiB）
/// # Returns
/// * `Result<VaultBootstrap, AppError>` - 创建并保存在本地的引导配置
#[tauri::command]
#[specta::specta]
pub async fn cmd_initialize_new_vault(
    state: State<'_, AppState>,
    master_password: String,
    memory_cost_kib: u32,
) -> Result<VaultBootstrap, AppError> {
    let _storage_mode_guard = state
        .try_lock_storage_mode_change()
        .ok_or_else(|| AppError {
            error_type: "storage_busy".into(),
            message: "有存储操作正在进行，请等待完成后再初始化 Vault".into(),
        })?;
    if !state
        .local_object_store()
        .get_all_entries()
        .await
        .map_err(AppError::from)?
        .is_empty()
    {
        return Err(super::VaultBootstrapError::ExistingLocalData.into());
    }
    Ok(state
        .vault_bootstrap_repository()
        .initialize_new(master_password, memory_cost_kib)?)
}

/// 导出可复制的密钥派生引导配置 JSON。内容不包含主密码或派生密钥。
/// # Returns
/// * `Result<String, AppError>` - 格式化后的 JSON
#[tauri::command]
#[specta::specta]
pub fn cmd_export_vault_bootstrap(state: State<'_, AppState>) -> Result<String, AppError> {
    Ok(state.vault_bootstrap_repository().export_json()?)
}

/// 导入密钥派生引导配置。只有配置能通过主密码校验且与当前已解锁密钥一致时才会保存。
/// # Arguments
/// * `json` - 从其他设备复制的完整引导配置 JSON
/// * `master_password` - 用于验证导入配置的当前主密码
/// # Returns
/// * `Result<VaultBootstrap, AppError>` - 验证并保存后的配置
#[tauri::command]
#[specta::specta]
pub async fn cmd_import_vault_bootstrap(
    state: State<'_, AppState>,
    json: String,
    master_password: String,
) -> Result<VaultBootstrap, AppError> {
    let _storage_guard = state.lock_storage_operation().await;
    Ok(state
        .vault_bootstrap_repository()
        .import_json(&json, master_password)
        .await?)
}

/// 首次在当前设备连接云端 Vault 时，先读取并验证云端引导配置，再建立解密密钥。
/// 旧桶没有引导配置时，会使用当前编译模式对应的旧版参数验证一份已有加密对象。
/// # Arguments
/// * `master_password` - 主密码
/// * `akid` - 访问密钥 ID
/// * `aks` - 访问密钥 Secret
/// * `bucket` - 存储桶名称
/// * `endpoint` - OSS 端点
/// * `new_vault_memory_cost_kib` - 仅在云端和本地都为空时用于创建新 Vault
/// # Returns
/// * `Result<VaultBootstrap, AppError>` - 已验证并保存在本地的引导配置
#[tauri::command]
#[specta::specta]
pub async fn cmd_prepare_remote_vault(
    state: State<'_, AppState>,
    master_password: String,
    akid: String,
    aks: String,
    bucket: String,
    endpoint: String,
    new_vault_memory_cost_kib: u32,
) -> Result<VaultBootstrap, AppError> {
    let _storage_mode_guard = state
        .try_lock_storage_mode_change()
        .ok_or_else(|| AppError {
            error_type: "storage_busy".into(),
            message: "有存储操作正在进行，请等待完成后再连接云端 Vault".into(),
        })?;
    let may_create_new_vault = state.vault_bootstrap().is_none()
        && state
            .local_object_store()
            .get_all_entries()
            .await
            .map_err(AppError::from)?
            .is_empty();
    state
        .oss_client()
        .initialize(endpoint, akid, aks, bucket)
        .map_err(AppError::from)?;
    match state
        .vault_bootstrap_repository()
        .prepare_remote(
            master_password,
            new_vault_memory_cost_kib,
            may_create_new_vault,
        )
        .await
    {
        Ok(bootstrap) => Ok(bootstrap),
        Err(error) => {
            log::warn!("[vault bootstrap] preparing remote vault failed: {error}");
            state.oss_client().reset();
            Err(error.into())
        }
    }
}
