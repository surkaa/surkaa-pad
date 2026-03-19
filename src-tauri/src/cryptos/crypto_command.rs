use tauri::State;
use crate::state::AppState;

/// 解锁加密管理器
/// # Arguments
/// * `master_password` - 主密码
/// * `salt` - 盐值
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_unlock(
    state: State<'_, AppState>,
    master_password: String,
    salt: String,
) -> Result<String, String> {
    state.crypto().derive_dek(master_password, salt)
}

/// 生物解锁，传入dek解锁
/// # Arguments
/// * `dek` - 数据加密密钥
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_biometric_unlock(state: State<'_, AppState>, dek: String) -> Result<(), String> {
    state.crypto().init_by_dek_string(dek)
}

/// 加密数据
/// # Arguments
/// * `data` - 待加密的数据
/// # Returns
/// * `Result<Vec<u8>, String>` - 成功时返回密文（包含nonce在首），失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn cmd_encrypt_data(state: State<'_, AppState>, data: String) -> Result<Vec<u8>, String> {
    state.crypto().encrypt(data.as_bytes())
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
) -> Result<String, String> {
    let decrypted_bytes = state.crypto().decrypt(&encrypted)?;
    let decrypted_string = String::from_utf8(decrypted_bytes).map_err(|e| e.to_string())?;
    Ok(decrypted_string)
}
