use crate::cryptos::crypto_types::MEMORY_COST_KIB;
use crate::error::AppError;
use crate::state::AppState;
use crate::vault_bootstrap::KeyDerivationParameters;
use tauri::State;

/// 解锁加密管理器
/// # Arguments
/// * `master_password` - 主密码
/// # Returns
/// * `Result<(), AppError>` - 成功时完成解锁，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_unlock(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<(), AppError> {
    let parameters = state
        .vault_bootstrap()
        .map(|bootstrap| bootstrap.kdf)
        .unwrap_or_else(KeyDerivationParameters::legacy_current);
    Ok(state
        .crypto()
        .derive_dek_with_parameters(master_password, parameters)?)
}

/// 验证密码获取密钥
/// # Arguments
/// * `master_password` - 主密码
/// # Returns
/// * `Result<String, AppError>` - 成功时返回数据加密密钥，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_valid_password(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<String, AppError> {
    Ok(state.crypto().valid_password(master_password)?)
}

/// 生物解锁，传入dek解锁
/// # Arguments
/// * `dek` - 数据加密密钥
/// # Returns
/// * `Result<(), AppError>` - 成功时完成解锁，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_biometric_unlock(state: State<'_, AppState>, dek: String) -> Result<(), AppError> {
    let parameters = state
        .vault_bootstrap()
        .map(|bootstrap| bootstrap.kdf)
        .unwrap_or_else(KeyDerivationParameters::legacy_current);
    Ok(state
        .crypto()
        .init_by_dek_string_with_parameters(dek, parameters)?)
}

/// 加密数据
/// # Arguments
/// * `data` - 待加密的数据
/// # Returns
/// * `Result<Vec<u8>, AppError>` - 成功时返回包含 nonce 的密文，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_encrypt_data(
    state: State<'_, AppState>,
    data: String,
) -> Result<Vec<u8>, AppError> {
    Ok(state.crypto().encrypt(data.as_bytes())?)
}

/// 解密数据
/// # Arguments
/// * `encrypted` - 密文
/// # Returns
/// * `Result<String, AppError>` - 成功时返回 UTF-8 明文，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_decrypt_data(
    state: State<'_, AppState>,
    encrypted: Vec<u8>,
) -> Result<String, AppError> {
    let decrypted_bytes = state.crypto().decrypt(&encrypted)?;
    let decrypted_string = String::from_utf8(decrypted_bytes).map_err(|e| AppError {
        error_type: "utf8".into(),
        message: e.to_string(),
    })?;
    Ok(decrypted_string)
}

/// 获取加密配置
/// # Returns
/// * `Result<u32, AppError>` - Argon2 内存成本（KiB）
#[tauri::command]
#[specta::specta]
pub async fn cmd_encrypt_info(state: State<'_, AppState>) -> Result<u32, AppError> {
    Ok(state
        .vault_bootstrap()
        .map(|bootstrap| bootstrap.kdf.memory_cost_kib)
        .unwrap_or(MEMORY_COST_KIB))
}
