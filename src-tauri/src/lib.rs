use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{
    password_hash::SaltString, Algorithm, Argon2, ParamsBuilder, PasswordHasher, Version,
};
use rand::RngCore;
use std::fs;
// 引入 Arc 用于跨线程共享所有权
use std::sync::Mutex;
use std::collections::HashMap;

use hmac::digest::core_api::{CoreWrapper, CtVariableCoreWrapper};
use hmac::digest::typenum::{UInt, UTerm, B0, B1};
use hmac::{HmacCore, Mac};
// 用于 HMAC
use sha2::{OidSha256, Sha256, Sha256VarCore};
use digest::Digest;

use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Serialize, Deserialize};
use tauri::{Manager, State};
use jieba_rs::Jieba;

// 定义常量
const NONCE_LEN: usize = 12;
// 定义派生密钥的长度（字节），AES-256 需要 32 字节
const KEY_LEN: usize = 32;
const HASH_LEN: usize = 32; // SHA256 哈希长度
const ALGORITHM_NAME: &str = "AES256-GCM_v1";

// ---------------------------------------------------------
// 结构体定义
// ---------------------------------------------------------

pub struct DbConnection(pub Mutex<Connection>);

// 返回给前端的结果
#[derive(Debug, Serialize)]
pub struct DiaryMeta {
    id: i64,
    nonce: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct KeywordToken {
    pub word: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct SearchIndexResult {
    pub id: i64,
    pub count: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct EncryptData {
    pub total_length: u16,
    pub algorithm: String, // 例如: "AES256-GCM_v1"
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub enc_hash: Vec<u8>,
}

// 前端批量传入的索引条目
#[derive(Debug, Deserialize)]
pub struct BatchIndexEntry {
    pub id: i64,
    pub search_hash: Vec<u8>,
    pub count: i64,
}

// ---------------------------------------------------------
// 核心逻辑函数
// ---------------------------------------------------------

#[tauri::command]
async fn derive_key(password: &str, salt: &str) -> Result<Vec<u8>, String> {
    // 1. 定义 Argon2 参数 (Params 只需要在这里创建一次)
    let memory_cost_kib = 1024 * 256;

    let params = ParamsBuilder::new()
        .t_cost(2)
        .m_cost(memory_cost_kib)
        .p_cost(4)
        .output_len(KEY_LEN)
        .build()
        .map_err(|e| format!("Argon2 参数错误: {}", e))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let salt = SaltString::from_b64(salt)
        .map_err(|e| format!("Salt 字符串无效或不是 Base64 编码: {}", e))?;

    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("密钥派生失败: {}", e))?;

    let dek = hash.hash.ok_or_else(|| "无法提取哈希值".to_string())?;

    if dek.as_bytes().len() != KEY_LEN {
        return Err(format!(
            "派生密钥长度错误: 期望 {}, 得到 {}",
            KEY_LEN,
            dek.as_bytes().len()
        ));
    }

    Ok(dek.as_bytes().to_vec())
}

#[tauri::command]
fn encrypt_data(dek: Vec<u8>, plaintext: String) -> Result<EncryptData, String> {
    let cipher = Aes256Gcm::new_from_slice(&dek).map_err(|_| "DEK 长度错误".to_string())?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("加密失败: {:?}", e))?;

    // --- 2. 计算加密内容哈希 (enc_hash) ---
    let mut hasher = Sha256::new();
    // 关键：哈希 Nonce + Ciphertext
    hasher.update(&nonce_bytes);
    hasher.update(&ciphertext);
    let enc_hash = hasher.finalize().to_vec();

    // --- 3. 计算文件头总长度 (total_length) ---
    // 2 (Length) + 1 (AlgoLen) + N (AlgoName) + 12 (IV) + 32 (Hash)
    let algo_name_len = ALGORITHM_NAME.len();
    let total_length: u16 = (2 + 1 + algo_name_len + NONCE_LEN + HASH_LEN)
        .try_into()
        .map_err(|_| "文件头长度超出了 u16 限制".to_string())?;

    // --- 4. 返回所有数据 ---
    Ok(EncryptData {
        total_length,
        algorithm: ALGORITHM_NAME.to_string(),
        nonce: nonce_bytes.to_vec(),
        ciphertext,
        enc_hash,
    })
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
        id          INTEGER NOT NULL,
        search_hash BLOB    NOT NULL,
        count       INTEGER NOT NULL,
        PRIMARY KEY (id, search_hash)
    )",
        (),
    )?;

    Ok(conn)
}

#[tauri::command]
fn save_keyword_index_batch(
    db_state: State<'_, DbConnection>,
    entries: Vec<BatchIndexEntry>,
) -> Result<(), String> {
    let mut conn = db_state.0.lock().unwrap();

    // 使用事务以提高写入性能
    let tx = conn.transaction().map_err(|e| format!("启动事务失败: {}", e))?;

    // 使用 INSERT OR IGNORE 来处理复合主键冲突，实现避免重复数据（去重）
    let sql = "INSERT OR IGNORE INTO index_hashes (id, search_hash, count) VALUES (?, ?, ?)";
    for entry in entries {
        // 批量执行插入
        tx.execute(
            sql,
            params![entry.id, entry.search_hash, entry.count],
        ).map_err(|e| format!("插入索引失败: id={} error={}", entry.id, e))?;
    }

    tx.commit().map_err(|e| format!("提交事务失败: {}", e))?;

    Ok(())
}

#[tauri::command]
fn search_local_index(
    db_state: State<'_, DbConnection>,
    search_hash: Vec<u8>,
) -> Result<Vec<SearchIndexResult>, String> {
    let conn = db_state.0.lock().map_err(|e| format!("锁失败: {}", e))?;

    let mut stmt = conn
        .prepare("SELECT id, count FROM index_hashes WHERE search_hash = ?1 ORDER BY count DESC")
        .map_err(|e| format!("准备失败: {}", e))?;

    let results_iter = stmt
        .query_map(params![search_hash], |row| {
            let id: i64 = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok(SearchIndexResult { id, count})
        })
        .map_err(|e| format!("执行失败: {}", e))?;

    let mut results = Vec::new();
    for result in results_iter {
        results.push(result.map_err(|e| format!("处理结果失败: {}", e))?);
    }

    Ok(results)
}

#[tauri::command]
fn tokenize_and_count(plaintext: String) -> Result<Vec<KeywordToken>, String> {
    // 1. 初始化分词器
    let jieba = Jieba::new();

    // 2. 分词 (使用 Search 模式)
    // 自动切分出长词和短词的组合
    let tokens = jieba.cut_for_search(&plaintext, true);

    // 3. 词频统计
    let mut word_counts: HashMap<String, i64> = HashMap::new();

    for token in tokens {
        let word = token.to_lowercase();
        // 过滤掉纯数字、单个字符和空白/标点符号，避免无效索引
        if word.chars().all(|c| c.is_ascii_whitespace() || c.is_ascii_punctuation() || c.is_digit(10)) || word.chars().count() < 2 {
            continue;
        }

        *word_counts.entry(word).or_insert(0) += 1;
    }

    // 4. 格式化返回结果
    let results: Vec<KeywordToken> = word_counts
        .into_iter()
        .map(|(word, count)| KeywordToken { word, count })
        .collect();

    // 降序排序
    let mut sorted_results = results;
    sorted_results.sort_by(|a, b| b.count.cmp(&a.count));

    Ok(sorted_results)
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
            save_keyword_index_batch,
            search_local_index,
            tokenize_and_count
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
