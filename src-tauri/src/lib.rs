// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

use argon2::{password_hash::SaltString, Argon2, Params, PasswordHasher};

// 定义派生密钥的长度（字节），AES-256 需要 32 字节
const KEY_LEN: usize = 32;

#[tauri::command]
fn derive_key(password: &str, salt: &str) -> Result<String, String> {
    // 1. 定义 Argon2 参数 (Params 只需要在这里创建一次)
    let params = Params::new(
        2,
        1024 * 64,
        4,
        Some(KEY_LEN),
    ).map_err(|e| format!("Argon2 参数错误: {}", e))?;

    // 2. 创建 Argon2 实例 (params 的所有权被移动到这里)
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params, // <-- params 所有权被移动
    );

    // 3. 将 salt 字符串解析为 SaltString (使用推荐的 from_b64)
    // 确保从前端传入的 salt 是 Base64 格式
    let salt = SaltString::from_b64(&salt)
        .map_err(|e| format!("Salt 字符串无效或不是 Base64 编码: {}", e))?;

    // 4. 执行派生 (使用正确的 hash_password API)
    // 注意：这里的 argon2 实例已经包含了 params，所以不需要再次传入。
    let hash = argon2.hash_password(
        password.as_bytes(),
        &salt,
    ).map_err(|e| format!("密钥派生失败: {}", e))?;

    // 5. 提取派生密钥 (DEK)
    let dek = hash.hash.ok_or_else(|| "无法提取哈希值".to_string())?;

    // 6. 确认 DEK 长度并返回
    if dek.as_bytes().len() != KEY_LEN {
        return Err(format!("派生密钥长度错误，预期 {} 字节，实际 {}", KEY_LEN, dek.as_bytes().len()));
    }

    Ok(String::try_from(dek.as_bytes().to_vec()).unwrap())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, derive_key])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
