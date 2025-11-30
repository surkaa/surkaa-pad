pub mod surkaa_pad;
pub mod encryption_manager;
pub mod oss_client_manager;
pub mod secure_diary_store;

use aes_gcm::aead::{KeyInit};
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

use hmac::digest::core_api::{CoreWrapper, CtVariableCoreWrapper};
use hmac::digest::typenum::{UInt, UTerm, B0, B1};
use hmac::{HmacCore, Mac};
use sha2::{OidSha256, Sha256VarCore};

use crate::encryption_manager::EncryptionManager;
use crate::oss_client_manager::{OssClientManager, OssError};
use jieba_rs::Jieba;
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
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
// Tauri 错误转换器
fn map_oss_err<T>(res: Result<T, OssError>) -> Result<T, String> {
    res.map_err(|e| e.to_string())
}

#[tauri::command]
async fn derive_key(
    encryption: State<'_, Mutex<EncryptionManager>>,
    password: &str,
    salt: &str,
) -> Result<(), String> {
    let mut encryption = encryption
        .lock()
        .map_err(|e| format!("无法锁定 EncryptionManager: {}", e))?;

    encryption.initial(password, salt).expect("无法派生密钥");

    Ok(())
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
    let tx = conn
        .transaction()
        .map_err(|e| format!("启动事务失败: {}", e))?;

    // 使用 INSERT OR IGNORE 来处理复合主键冲突，实现避免重复数据（去重）
    let sql = "INSERT OR IGNORE INTO index_hashes (id, search_hash, count) VALUES (?, ?, ?)";
    for entry in entries {
        // 批量执行插入
        tx.execute(sql, params![entry.id, entry.search_hash, entry.count])
            .map_err(|e| format!("插入索引失败: id={} error={}", entry.id, e))?;
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
            Ok(SearchIndexResult { id, count })
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
        if word
            .chars()
            .all(|c| c.is_ascii_whitespace() || c.is_ascii_punctuation() || c.is_digit(10))
            || word.chars().count() < 2
        {
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

#[tauri::command]
async fn initialize_oss(
    client_state: State<'_, OssClientManager>,
    access_key_id: String,
    access_key_secret: String,
    region: String,
    bucket: String,
) -> Result<(), String> {
    let res = client_state
        .initialize(&access_key_id, &access_key_secret, &region, &bucket)
        .await;
    map_oss_err(res)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 新增: 注册 Store 插件
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(Mutex::new(EncryptionManager::new()))
        .manage(OssClientManager::default())
        .setup(|app| {
            let conn = init_db(&app.handle()).expect("无法初始化数据库连接");
            app.manage(DbConnection(Mutex::new(conn)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            derive_key,
            generate_search_hash,
            save_keyword_index_batch,
            search_local_index,
            tokenize_and_count,
            initialize_oss,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
