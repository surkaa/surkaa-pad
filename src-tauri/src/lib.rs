const OSS_ENDPOINT: &str = "oss-cn-guangzhou.aliyuncs.com";
const OSS_BUCKET: &str = "surkaa";

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{password_hash::SaltString, Argon2, ParamsBuilder, PasswordHasher};
use rand::RngCore;
// 用于生成随机 IV

// 定义 IV 长度 (AES-GCM 标准 IV 长度为 12 字节)
const NONCE_LEN: usize = 12;
// 定义派生密钥的长度（字节），AES-256 需要 32 字节
const KEY_LEN: usize = 32;
use hmac::digest::core_api::{CoreWrapper, CtVariableCoreWrapper};
use hmac::digest::typenum::{UInt, UTerm, B0, B1};
use hmac::{Hmac, HmacCore, Mac};
// 用于 HMAC
use sha2::{OidSha256, Sha256, Sha256VarCore};
// HMAC 使用的哈希算法

use ali_oss_rs::object::ObjectOperations;
use ali_oss_rs::object_common::PutObjectOptionsBuilder;
use ali_oss_rs::Client;
use tauri::Manager;
use tauri::State;
// 导入 State

// 定义 HMAC 实例类型
type HmacSha256 = Hmac<Sha256>;
#[tauri::command]
fn derive_key(password: &str, salt: &str) -> Result<Vec<u8>, String> {
    // 1. 定义 Argon2 参数 (Params 只需要在这里创建一次)
    let memory_cost_kib = 1024 * 256;

    let params = ParamsBuilder::new()
        .t_cost(2) // 迭代次数 (Time cost)
        .m_cost(memory_cost_kib) // 内存消耗 (256 MiB)
        .p_cost(4) // 并行度 (Parallelism)
        .output_len(KEY_LEN) // 密钥长度 (32 字节)
        .build()
        .map_err(|e| format!("Argon2 参数错误: {}", e))?;

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
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("密钥派生失败: {}", e))?;

    // 5. 提取派生密钥 (DEK)
    let dek = hash.hash.ok_or_else(|| "无法提取哈希值".to_string())?;

    // 6. 确认 DEK 长度并返回
    if dek.as_bytes().len() != KEY_LEN {
        return Err(format!(
            "派生密钥长度错误，预期 {} 字节，实际 {}",
            KEY_LEN,
            dek.as_bytes().len()
        ));
    }

    Ok(dek.as_bytes().to_vec())
}

/// 使用 DEK 对数据进行加密
/// 返回 (密文, IV)
#[tauri::command]
fn encrypt_data(dek: Vec<u8>, plaintext: String) -> Result<(Vec<u8>, Vec<u8>), String> {
    // 1. 初始化加密器
    // 注意：Aes256Gcm::new 期望一个 32 字节的 Key
    let cipher = Aes256Gcm::new_from_slice(&dek)
        .map_err(|_| "DEK 长度错误，无法初始化加密器".to_string())?;

    // 2. 生成随机 Nonce (即 IV)
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // 3. 执行加密
    // 加密结果包含了密文和 GCM Tag (T)
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("加密失败: {:?}", e))?;

    // 返回密文和 IV。GCM Tag 包含在 ciphertext 的末尾
    Ok((ciphertext, nonce_bytes.to_vec()))
}

/// 使用 DEK 和 IV 对密文进行解密
/// 返回 明文 (String)
#[tauri::command]
fn decrypt_data(dek: Vec<u8>, ciphertext: Vec<u8>, nonce_bytes: Vec<u8>) -> Result<String, String> {
    // 1. 初始化加密器
    let cipher = Aes256Gcm::new_from_slice(&dek)
        .map_err(|_| "DEK 长度错误，无法初始化加密器".to_string())?;

    // 2. 准备 Nonce (IV)
    if nonce_bytes.len() != NONCE_LEN {
        return Err("IV 长度不正确".to_string());
    }
    let nonce = Nonce::from_slice(&nonce_bytes);

    // 3. 执行解密
    // 如果 ciphertext 被篡改或 IV/DEK 错误，解密会失败
    let decrypted_bytes = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| format!("解密失败或数据被篡改 (GCM Tag 不匹配): {:?}", e))?;

    // 4. 转换为字符串并返回
    String::from_utf8(decrypted_bytes)
        .map_err(|_| "解密后的字节不是有效的 UTF-8 字符串".to_string())
}

#[tauri::command]
fn generate_search_hash(dek: Vec<u8>, keyword: String) -> Result<Vec<u8>, String> {
    // 1. 初始化 HMAC 实例 (DEK 作为密钥)
    let mut mac = <CoreWrapper<
        HmacCore<
            CoreWrapper<
                CtVariableCoreWrapper<
                    Sha256VarCore,
                    UInt<UInt<UInt<UInt<UInt<UInt<UTerm, B1>, B0>, B0>, B0>, B0>, B0>,
                    OidSha256,
                >,
            >,
        >,
    > as KeyInit>::new_from_slice(&dek)
    .map_err(|_| "DEK 长度错误，无法初始化 HMAC".to_string())?;

    // 2. 更新 MAC (计算关键词的哈希)
    mac.update(keyword.as_bytes());

    // 3. 提取结果 (即加密指纹)
    let result = mac.finalize();
    let code_bytes = result.into_bytes();

    // HMAC-SHA256 总是生成 32 字节的输出
    Ok(code_bytes.to_vec())
}

// 定义一个结构体来存储 OSS 客户端
pub struct OssClient(pub Client);

/// 客户端初始化函数 (在应用启动/登录时调用)
#[tauri::command]
async fn initialize_oss_client(
    app_handle: tauri::AppHandle, // 用于获取应用状态
    ak_id: String,
    ak_secret: String,
    endpoint: String,
    bucket: String,
) -> Result<(), String> {

    // 1. 创建 OSS 客户端实例
    let client = Client::new(
        ak_id,
        ak_secret,
        endpoint,
        bucket,
    );

    // 2. 将客户端存储到 Tauri 状态管理器中
    // 检查是否已经存储过，如果是，则更新
    app_handle.manage(OssClient(client));

    // 成功后返回
    Ok(())
}

/// 任务 4.2：上传加密数据到 OSS
#[tauri::command]
async fn upload_diary(
    client_state: State<'_, OssClient>,
    bucket_name: String, // <-- 修正：将 bucket_name 作为参数传入
    object_key: String, // OSS 路径，例如: data/{user_id}/{entry_id}.dat
    encrypted_data: Vec<u8>, // 加密后的 Vec<u8> (密文 + IV + Tag)
) -> Result<(), String> {

    let client = &client_state.0;

    // 1. 设置 PutObjectOptions (可选，但推荐设置 Content-Type)
    // 我们的文件是原始二进制数据 (.dat)，设置为 application/octet-stream
    let options = PutObjectOptionsBuilder::new()
        .mime_type("application/octet-stream")
        .forbid_overwrite(true)
        .build();

    // 2. 执行 Put Object (Put object from buffer)
    // 修正点：添加 bucket_name 和 options 参数
    client
        .put_object_from_buffer(
            &bucket_name, // 传入 bucket_name
            &object_key,
            encrypted_data, // buffer: B: Into<Vec<u8>>
            Some(options), // options: Option<PutObjectOptions>
        )
        .await
        .map_err(|e| format!("OSS 上传失败: {}", e))?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 在这里初始化一个默认状态，稍后会被 initialize_oss_client 替换
        .manage(OssClient(Client::new(
            "dummy".to_string(), "dummy".to_string(), "dummy".to_string(), "dummy".to_string()
        )))
        .invoke_handler(tauri::generate_handler![
            derive_key,
            encrypt_data,
            decrypt_data,
            generate_search_hash,
            initialize_oss_client,
            upload_diary
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
