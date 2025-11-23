use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{password_hash::SaltString, Argon2, ParamsBuilder, PasswordHasher};
use rand::RngCore;
use std::fs;
// 引入 Arc 用于跨线程共享所有权
use std::sync::{Arc, Mutex};
// 用于生成随机 IV

use hmac::digest::core_api::{CoreWrapper, CtVariableCoreWrapper};
use hmac::digest::typenum::{UInt, UTerm, B0, B1};
use hmac::{HmacCore, Mac};
// 用于 HMAC
use sha2::{OidSha256, Sha256VarCore};
// HMAC 使用的哈希算法

use rusqlite::{Connection, Result as SqlResult};
use serde::Serialize;
use tauri::Manager;
use tauri::State;

// 定义常量
const NONCE_LEN: usize = 12;
// 定义派生密钥的长度（字节），AES-256 需要 32 字节
const KEY_LEN: usize = 32;

// ---------------------------------------------------------
// 结构体定义
// ---------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SearchResult {
    entry_id: String,
    nonce: Vec<u8>, // IV，用于解密日记内容
    created_at: String,
}

// 新增：日记列表项结构体
#[derive(Debug, Serialize)]
pub struct DiaryMeta {
    entry_id: String,
    created_at: String,
    nonce: Vec<u8>,
}

pub struct DbConnection(pub std::sync::Mutex<Connection>);

// ---------------------------------------------------------
// 核心逻辑函数
// ---------------------------------------------------------

#[tauri::command]
fn derive_key(password: &str, salt: &str) -> Result<Vec<u8>, String> {
    // 1. 定义 Argon2 参数 (Params 只需要在这里创建一次)
    let memory_cost_kib = 1024 * 256;

    let params = ParamsBuilder::new()
        .t_cost(2)
        .m_cost(memory_cost_kib)
        .p_cost(4)
        .output_len(KEY_LEN)
        .build()
        .map_err(|e| format!("Argon2 参数错误: {}", e))?;

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let salt = SaltString::from_b64(&salt)
        .map_err(|e| format!("Salt 字符串无效或不是 Base64 编码: {}", e))?;

    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("密钥派生失败: {}", e))?;

    let dek = hash.hash.ok_or_else(|| "无法提取哈希值".to_string())?;

    if dek.as_bytes().len() != KEY_LEN {
        return Err(format!("派生密钥长度错误"));
    }

    Ok(dek.as_bytes().to_vec())
}

#[tauri::command]
fn encrypt_data(dek: Vec<u8>, plaintext: String) -> Result<(Vec<u8>, Vec<u8>), String> {
    let cipher = Aes256Gcm::new_from_slice(&dek).map_err(|_| "DEK 长度错误".to_string())?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("加密失败: {:?}", e))?;

    Ok((ciphertext, nonce_bytes.to_vec()))
}

#[tauri::command]
fn decrypt_data(dek: Vec<u8>, ciphertext: Vec<u8>, nonce_bytes: Vec<u8>) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(&dek).map_err(|_| "DEK 长度错误".to_string())?;

    if nonce_bytes.len() != NONCE_LEN {
        return Err("IV 长度不正确".to_string());
    }
    let nonce = Nonce::from_slice(&nonce_bytes);

    let decrypted_bytes = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| format!("解密失败: {:?}", e))?;

    String::from_utf8(decrypted_bytes).map_err(|_| "不是有效的 UTF-8".to_string())
}

// 配置加密与解密 (用于持久化存储)

/// 加密配置信息 (JSON 字符串) -> 返回 [IV + 密文] 的组合字节
#[tauri::command]
fn encrypt_config(dek: Vec<u8>, config_json: String) -> Result<Vec<u8>, String> {
    // 复用 encrypt_data
    let (ciphertext, iv) = encrypt_data(dek, config_json)?;

    // 将 IV 拼接到密文前面，方便存储
    let mut result = iv;
    result.extend(ciphertext);

    Ok(result)
}

/// 解密配置信息 [IV + 密文] -> JSON 字符串
#[tauri::command]
fn decrypt_config(dek: Vec<u8>, encrypted_data: Vec<u8>) -> Result<String, String> {
    if encrypted_data.len() < NONCE_LEN {
        return Err("数据长度不足".to_string());
    }

    // 拆分 IV 和密文
    let iv = encrypted_data[..NONCE_LEN].to_vec();
    let ciphertext = encrypted_data[NONCE_LEN..].to_vec();

    decrypt_data(dek, ciphertext, iv)
}

#[tauri::command]
fn generate_search_hash(dek: Vec<u8>, keyword: String) -> Result<Vec<u8>, String> {
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
    .map_err(|_| "DEK 长度错误".to_string())?;

    mac.update(keyword.as_bytes());
    let result = mac.finalize();
    Ok(result.into_bytes().to_vec())
}

fn init_db(app_handle: &tauri::AppHandle) -> SqlResult<Connection> {
    let app_data_dir = app_handle.path().app_data_dir().expect("无法获取数据目录");
    let db_path = app_data_dir.join("local_index.db");
    fs::create_dir_all(&app_data_dir).expect("无法创建目录");
    let conn = Connection::open(&db_path)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS index_hashes (
        entry_id    TEXT NOT NULL,
        nonce       BLOB NOT NULL,
        created_at  TEXT NOT NULL,
        search_hash BLOB NOT NULL,
        PRIMARY KEY (entry_id, search_hash)
    )",
        (),
    )?;

    Ok(conn)
}

#[tauri::command]
fn save_local_index(
    db_state: State<'_, DbConnection>,
    entry_id: String,
    nonce: Vec<u8>,
    created_at: String,
    search_hash: Vec<u8>,
) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| format!("锁失败: {}", e))?;

    conn.execute(
        "INSERT INTO index_hashes (entry_id, nonce, created_at, search_hash) VALUES (?1, ?2, ?3, ?4)",
        (entry_id, nonce, created_at, search_hash),
    )
        .map_err(|e| format!("写入失败: {}", e))?;

    Ok(())
}

#[tauri::command]
fn search_local_index(
    db_state: State<'_, DbConnection>,
    search_hash: Vec<u8>,
) -> Result<Vec<SearchResult>, String> {
    let conn = db_state.0.lock().map_err(|e| format!("锁失败: {}", e))?;

    let mut stmt = conn
        .prepare("SELECT entry_id, nonce, created_at FROM index_hashes WHERE search_hash = ?1")
        .map_err(|e| format!("准备失败: {}", e))?;

    let results_iter = stmt
        .query_map([search_hash], |row| {
            Ok(SearchResult {
                entry_id: row.get(0)?,
                nonce: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|e| format!("执行失败: {}", e))?;

    let mut results = Vec::new();
    for result in results_iter {
        results.push(result.map_err(|e| format!("处理结果失败: {}", e))?);
    }

    Ok(results)
}

// 获取所有日记列表 (用于手机端展示)
#[tauri::command]
fn get_all_entries(db_state: State<'_, DbConnection>) -> Result<Vec<DiaryMeta>, String> {
    let conn = db_state.0.lock().map_err(|e| format!("锁失败: {}", e))?;

    // 使用 DISTINCT 因为一个日记可能有多个关键词索引，我们只需要列出日记本身
    let mut stmt = conn
        .prepare("SELECT entry_id, created_at, nonce FROM index_hashes GROUP BY entry_id ORDER BY created_at DESC")
        .map_err(|e| format!("准备失败: {}", e))?;

    let results_iter = stmt
        .query_map([], |row| {
            Ok(DiaryMeta {
                entry_id: row.get(0)?,
                created_at: row.get(1)?,
                nonce: row.get(2)?,
            })
        })
        .map_err(|e| format!("执行失败: {}", e))?;

    let mut results = Vec::new();
    for result in results_iter {
        results.push(result.map_err(|e| format!("处理失败: {}", e))?);
    }

    Ok(results)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 新增: 注册 Store 插件
        .plugin(tauri_plugin_store::Builder::default().build())
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
            save_local_index,
            search_local_index,
            encrypt_config,
            decrypt_config,
            get_all_entries
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
