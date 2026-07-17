use crate::cryptos::crypto_types::{DERIVE_SALT, MEMORY_COST_KIB};
use crate::error::AppError;
use crate::state::AppState;
use tauri::State;

/// 解锁加密管理器
/// # Arguments
/// * `master_password` - 主密码
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_unlock(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<(), AppError> {
    Ok(state.crypto().derive_dek(master_password, DERIVE_SALT)?)
}

/// 验证密码获取密钥
/// # Arguments
/// * `master_password` - 主密码
/// # Returns
/// * `Result<String, String>` - 成功时返回数据加密密钥，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_valid_password(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<String, AppError> {
    Ok(state
        .crypto()
        .valid_password(master_password, DERIVE_SALT)?)
}

/// 生物解锁，传入dek解锁
/// # Arguments
/// * `dek` - 数据加密密钥
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_biometric_unlock(state: State<'_, AppState>, dek: String) -> Result<(), AppError> {
    Ok(state.crypto().init_by_dek_string(dek)?)
}

/// 加密数据
/// # Arguments
/// * `data` - 待加密的数据
/// # Returns
/// * `Result<Vec<u8>, String>` - 成功时返回密文（包含nonce在首），失败时返回错误信息
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
/// * `Result<Vec<u8>, String>` - 成功时返回明文，失败时返回错误信息
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
#[tauri::command]
#[specta::specta]
pub async fn cmd_encrypt_info() -> Result<u32, AppError> {
    Ok(MEMORY_COST_KIB)
}
