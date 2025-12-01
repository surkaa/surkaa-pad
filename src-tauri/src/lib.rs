pub mod encryption_manager;
pub mod oss_client_manager;
pub mod secure_diary_store;
pub mod surkaa_pad;

use crate::encryption_manager::EncryptionManager;
use crate::oss_client_manager::OssClientManager;
use crate::secure_diary_store::{DiaryManifest, SecureDiaryStore};
use crate::surkaa_pad::{AppState, DiaryMemoryCache};
use std::sync::{Arc, Mutex};
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
    em_state: State<'_, Arc<Mutex<EncryptionManager>>>,
    master_password: &str,
    salt: &str,
) -> Result<(), String> {
    // 获取 Mutex 锁，这会返回一个 MutexGuard
    let mut em = em_state
        .lock()
        .map_err(|e| format!("无法锁定 EncryptionManager: {}", e))?;

    // 在 MutexGuard 上调用方法。MutexGuard 实现了 DerefMut，允许可变访问
    em.initial(master_password, salt).map_err(|e| e.to_string())
}

/// 列出本地缓存的日记列表
/// # Arguments
/// 无需手动传参数
/// # Returns
/// * `Result<Vec<DiaryManifest>, String>` - 成功时返回日记列表，失败时返回错误信息
#[tauri::command]
async fn list_local_list(
    cache: State<'_, DiaryMemoryCache>,
    encryption: State<'_, Arc<Mutex<EncryptionManager>>>,
    store: State<'_, SecureDiaryStore>,
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<Vec<DiaryManifest>, String> {
    // 获取 Mutex 锁，这会返回一个 MutexGuard
    let em = encryption
        .lock()
        .map_err(|e| format!("无法锁定 EncryptionManager: {}", e))?;
    app_state
        .load_cache_to_memory(&*cache, &*em, &*store, &app_handle)
        .await?;
    let diaries = app_state.list_cached_diaries(&*cache);
    Ok(diaries)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 注册 Store 插件
        .plugin(Builder::default().build())
        .setup(|app| {
            app.manage(Arc::new(Mutex::new(EncryptionManager::new())));
            app.manage(OssClientManager::default());
            app.manage(SecureDiaryStore::default());
            app.manage(DiaryMemoryCache::new());
            app.manage(AppState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![unlock, list_local_list,])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
