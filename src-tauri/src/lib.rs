pub mod encryption_manager;
pub mod oss_client_manager;
pub mod secure_diary_store;
pub mod surkaa_pad;

use crate::encryption_manager::EncryptionManager;
use crate::oss_client_manager::OssClientManager;
use crate::secure_diary_store::{DiaryManifest, SecureDiaryStore};
use crate::surkaa_pad::{AppState, DiaryMemoryCache};
use std::ops::Deref;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_store::Builder;

//------------
// 解锁与加密解密 以及初始化云端存储客户端
// TODO 未来是否可以直接在rust层操作配置文件？免去向前端直接提供加密与解密的命令
//------------

/// 解锁加密管理器
/// # Arguments
/// * `master_password` - 主密码
/// * `salt` - 盐值
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn unlock(
    em_state: State<'_, EncryptionManager>,
    master_password: &str,
    salt: &str,
) -> Result<(), String> {
    em_state.initial(master_password, salt).await
}

/// 加密数据
/// # Arguments
/// * `data` - 待加密的数据
/// # Returns
/// * `Result<(Vec<u8>, Vec<u8>), String>` - 成功时返回 (密文, nonce)，失败时返回错误信息
#[tauri::command]
async fn encrypt_data(
    em_state: State<'_, EncryptionManager>,
    data: String,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    em_state.encrypt(&data.as_bytes()).await
}

/// 解密数据
/// # Arguments
/// * `ciphertext` - 密文
/// * `nonce` - 解密用的 nonce
/// # Returns
/// * `Result<Vec<u8>, String>` - 成功时返回明文，失败时返回错误信息
#[tauri::command]
async fn decrypt_data(
    em_state: State<'_, EncryptionManager>,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
) -> Result<String, String> {
    let decrypted_bytes = em_state.decrypt(&ciphertext, &nonce).await?;
    let decrypted_string = String::from_utf8(decrypted_bytes).map_err(|e| e.to_string())?;
    Ok(decrypted_string)
}

/// 初始化 OSS 客户端
/// # Arguments
/// * `access_key_id` - 访问密钥 ID
/// * `access_key_secret` - 访问密钥 Secret
/// * `endpoint_name` - OSS 端点
/// * `bucket` - 存储桶名称
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn init_oss_client(
    client_state: State<'_, OssClientManager>,
    access_key_id: &str,
    access_key_secret: &str,
    endpoint_name: &str,
    bucket: &str,
) -> Result<(), String> {
    client_state
        .initialize(
            access_key_id,
            access_key_secret,
            endpoint_name,
            bucket,
        )
        .await
        .map_err(|e| e.to_string())
}

//------------
// 日记查询与同步
//------------

/// 列出本地缓存的日记列表
/// # Arguments
/// 无需手动传参数
/// # Returns
/// * `Result<Vec<DiaryManifest>, String>` - 成功时返回日记列表，失败时返回错误信息
#[tauri::command]
async fn list_local_list(
    cache: State<'_, DiaryMemoryCache>,
    em: State<'_, EncryptionManager>,
    store: State<'_, SecureDiaryStore>,
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<Vec<DiaryManifest>, String> {
    app_state
        .load_cache_to_memory(cache.deref(), em.deref(), store.deref(), &app_handle)
        .await?;
    let diaries = app_state.list_cached_diaries(cache.deref()).await;
    Ok(diaries)
}

/// 从 OSS 同步日记到本地缓存
/// # Arguments
/// 无需手动传参数
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn sync_from_oss(
    cache: State<'_, DiaryMemoryCache>,
    em: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    app_state
        .sync_from_oss(
            cache.deref(),
            em.deref(),
            client.deref(),
            store.deref(),
            &app_handle,
        )
        .await
}

/// 根据日记内容搜索日记
/// # Arguments
/// * `keyword` - 搜索关键词
/// # Returns
/// * `Result<Vec<DiaryManifest>, String>` - 成功时返回匹配的日记列表，失败时返回错误信息
#[tauri::command]
async fn search_diaries(
    cache: State<'_, &DiaryMemoryCache>,
    keyword: &str,
) -> Result<Vec<DiaryManifest>, String> {
    let map = cache.diaries.lock().await;
    let results: Vec<DiaryManifest> = map
        .values()
        .filter(|diary| diary.content.contains(keyword))
        .cloned()
        .collect();
    Ok(results)
}

//------------
// 日记操作相关
//------------

/// 根据内容保存日记
/// # Arguments
/// * `content` - 日记内容
/// # Returns
/// * `Result<String, String>` - 成功时返回日记 UUID，失败时返回错误信息
#[tauri::command]
async fn save_diary(
    encryption: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    content: &str,
) -> Result<String, String> {
    store
        .create_diary(encryption.deref(), client.deref(), content)
        .await
}

/// 更新日记的内容
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `new_content` - 新的日记内容
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn update_diary_content_only(
    encryption: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    uuid: String,
    new_content: &str,
) -> Result<(), String> {
    store
        .update_diary_content_only(encryption.deref(), client.deref(), uuid, new_content)
        .await
}

/// 删除日记
/// # Arguments
/// * `uuid` - 日记 UUID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn delete_diary(
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    uuid: String,
) -> Result<(), String> {
    store.delete_diary(client.deref(), uuid).await
}

//------------
// 附件操作相关
//------------

/// 添加附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `attachment_bytes` - 附件字节数据
/// * `mine_type` - 附件 MIME 类型
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn add_attachment(
    encryption: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    uuid: String,
    attachment_bytes: Vec<u8>,
    mine_type: String,
) -> Result<(), String> {
    store
        .add_attachment(
            encryption.deref(),
            client.deref(),
            uuid,
            attachment_bytes,
            mine_type,
        )
        .await
}

/// 下载附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `file_name` - 附件 ID
/// * `nonce` - 解密iv
/// # Returns
/// * `Result<Vec<u8>, String>` - 成功时返回附件字节数据，失败时返回错误信息
#[tauri::command]
async fn download_attachment(
    encryption: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    uuid: String,
    file_name: String,
    nonce: Vec<u8>,
) -> Result<Vec<u8>, String> {
    store
        .download_attachment(encryption.deref(), client.deref(), uuid, file_name, nonce)
        .await
}

/// 删除附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `file_name` - 附件 ID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn delete_attachment(
    encryption: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    uuid: String,
    file_name: String,
) -> Result<(), String> {
    store
        .delete_attachment(encryption.deref(), client.deref(), uuid, file_name)
        .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 注册 Store 插件
        .plugin(Builder::default().build())
        .setup(|app| {
            app.manage(EncryptionManager::new());
            app.manage(OssClientManager::default());
            app.manage(SecureDiaryStore::default());
            app.manage(DiaryMemoryCache::new());
            app.manage(AppState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            unlock,
            encrypt_data,
            decrypt_data,
            init_oss_client,
            list_local_list,
            sync_from_oss,
            search_diaries,
            save_diary,
            update_diary_content_only,
            delete_diary,
            add_attachment,
            download_attachment,
            delete_attachment
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
