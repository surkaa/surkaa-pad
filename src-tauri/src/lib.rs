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
            list_local_list,
            sync_from_oss,
            search_diaries,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
