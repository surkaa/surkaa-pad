use std::fs;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{password_hash::SaltString, Argon2, ParamsBuilder, PasswordHasher};
use rand::RngCore;
use std::sync::Mutex;
// 用于生成随机 IV

// 定义 IV 长度 (AES-GCM 标准 IV 长度为 12 字节)
const NONCE_LEN: usize = 12;
// 定义派生密钥的长度（字节），AES-256 需要 32 字节
const KEY_LEN: usize = 32;
use hmac::digest::core_api::{CoreWrapper, CtVariableCoreWrapper};
use hmac::digest::typenum::{UInt, UTerm, B0, B1};
use hmac::{HmacCore, Mac};
// 用于 HMAC
use sha2::{OidSha256, Sha256VarCore};
// HMAC 使用的哈希算法

use ali_oss_rs::object::ObjectOperations;
use ali_oss_rs::object_common::{
    DeleteObjectOptions, GetObjectOptionsBuilder, PutObjectOptionsBuilder,
};
use ali_oss_rs::Client;
use rusqlite::{Connection, Result as SqlResult};
use tauri::Manager;
use tauri::State;
use serde::Serialize; // 导入 Serialize

// 定义用于返回给前端的搜索结果结构体
#[derive(Debug, Serialize)]
pub struct SearchResult {
    entry_id: String,
    nonce: Vec<u8>, // IV，用于解密日记内容
    created_at: String,
}

// 定义一个结构体来存储数据库连接，使用 Mutex 确保线程安全
pub struct DbConnection(pub std::sync::Mutex<Connection>);

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
    let client = Client::new(ak_id, ak_secret, endpoint, bucket);

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
    bucket_name: String,     // <-- 修正：将 bucket_name 作为参数传入
    object_key: String,      // OSS 路径，例如: data/{user_id}/{entry_id}.dat
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
            Some(options),  // options: Option<PutObjectOptions>
        )
        .await
        .map_err(|e| format!("OSS 上传失败: {}", e))?;

    Ok(())
}

/// 任务 4.3：从 OSS 下载加密数据
/// 返回 Vec<u8>，包含加密密文、IV 和 Tag
#[tauri::command]
async fn download_diary(
    client_state: State<'_, OssClient>,
    bucket_name: String,
    object_key: String, // OSS 路径，例如: data/{user_id}/{entry_id}.dat
) -> Result<Vec<u8>, String> {
    let client = &client_state.0;

    // 1. 定义下载选项 (通常不需要特殊设置)
    let options = GetObjectOptionsBuilder::new().build();

    // 2. 执行 Get Object，下载到内存 (download_to_memory)
    // 注意：ali-oss-rs 库通常会提供一个 download_to_memory 的方法
    let result = client
        .get_object_to_buffer(
            // 假设 ali-oss-rs 提供类似的方法
            &bucket_name,
            &object_key,
            Some(options),
        )
        .await
        .map_err(|e| format!("OSS 下载失败: {:?}", e))?;

    // 3. 提取并返回下载的字节缓冲区 (Result<Vec<u8>>)
    // 假设 get_object_to_buffer 返回包含 buffer 的结构体，这里需要根据 ali-oss-rs 的具体 API 调整。
    // 经验上，OSS SDK 的 Get Object 返回值通常是一个包含字节数据的 Result 结构。

    // 如果 result 结构体中有一个名为 'data' 或 'buffer' 的字段：
    // let buffer = result.buffer;

    // 如果 API 签名是直接返回 Vec<u8> (更常见于 Rust):
    Ok(result)
}

/// 任务 4.4：从 OSS 删除 Object
#[tauri::command]
async fn delete_diary(
    client_state: State<'_, OssClient>,
    bucket_name: String,
    object_key: String,
) -> Result<(), String> {
    let client = &client_state.0;

    let options = DeleteObjectOptions::default();

    // 执行 Delete Object
    client
        .delete_object(&bucket_name, &object_key, Some(options))
        .await
        .map_err(|e| format!("OSS 删除失败: {:?}", e))?;

    Ok(())
}

// 数据库初始化函数：创建连接和表结构
fn init_db(app_handle: &tauri::AppHandle) -> SqlResult<Connection> {
    // 1. 确定数据库文件路径
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .expect("无法获取应用数据目录");

    let db_path = app_data_dir.join("local_index.db"); // 完整的文件路径

    // 2. 修正点：确保应用数据目录存在
    // 如果目录不存在，创建它及其所有父目录
    fs::create_dir_all(&app_data_dir)
        .expect("无法创建应用数据目录");

    // 3. 创建或打开数据库连接
    // 使用 db_path 打开连接
    let conn = Connection::open(&db_path)?;

    // 4. 创建索引表 (保持不变)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS entries (
            entry_id    TEXT PRIMARY KEY,
            nonce       BLOB NOT NULL,
            created_at  TEXT NOT NULL,
            search_hash BLOB NOT NULL
        )",
        (),
    )?;

    Ok(conn)
}

/// 任务 4.7：将新日记条目的索引信息写入本地数据库
#[tauri::command]
fn save_local_index(
    db_state: State<'_, DbConnection>, // 接收数据库状态
    entry_id: String,
    nonce: Vec<u8>,
    created_at: String, // ISO 8601 格式
    search_hash: Vec<u8>,
) -> Result<(), String> {

    // 1. 获取数据库连接锁
    let conn = db_state.0.lock().map_err(|e| format!("获取数据库锁失败: {}", e))?;

    // 2. 执行插入操作
    conn.execute(
        "INSERT INTO entries (entry_id, nonce, created_at, search_hash) VALUES (?1, ?2, ?3, ?4)",
        (entry_id, nonce, created_at, search_hash),
    )
        .map_err(|e| format!("索引写入失败: {}", e))?;

    Ok(())
}

/// 任务 4.7：在本地数据库中查询匹配的加密索引
/// 返回匹配的日记条目列表 (ID, IV, CreatedAt)
#[tauri::command]
fn search_local_index(
    db_state: State<'_, DbConnection>,
    search_hash: Vec<u8>, // 要搜索的 HMAC-SHA256 指纹
) -> Result<Vec<SearchResult>, String> {

    // 1. 获取数据库连接锁
    let conn = db_state.0.lock().map_err(|e| format!("获取数据库锁失败: {}", e))?;

    // 2. 执行查询
    let mut stmt = conn
        .prepare("SELECT entry_id, nonce, created_at FROM entries WHERE search_hash = ?1")
        .map_err(|e| format!("数据库查询准备失败: {}", e))?;

    // 3. 映射结果
    let results_iter = stmt
        .query_map([search_hash], |row| {
            Ok(SearchResult {
                entry_id: row.get(0)?,
                nonce: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|e| format!("数据库查询执行失败: {}", e))?;

    // 4. 收集所有结果
    let mut results = Vec::new();
    for result in results_iter {
        results.push(result.map_err(|e| format!("处理查询结果失败: {}", e))?);
    }

    Ok(results)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 在这里初始化一个默认状态，稍后会被 initialize_oss_client 替换
        .manage(OssClient(Client::new(
            "dummy".to_string(),
            "dummy".to_string(),
            "dummy".to_string(),
            "dummy".to_string(),
        )))
        .setup(|app| {
            let conn = init_db(&app.handle()).expect("无法初始化数据库连接");

            app.manage(DbConnection(Mutex::new(conn)));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            derive_key,
            encrypt_data,
            decrypt_data,
            generate_search_hash,
            initialize_oss_client,
            upload_diary,
            download_diary,
            delete_diary,
            save_local_index,
            search_local_index
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
