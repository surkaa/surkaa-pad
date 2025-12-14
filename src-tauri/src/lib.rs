pub mod cache_file_manager;
pub mod encryption_manager;
pub mod oss_client_manager;
pub mod secure_diary_store;
pub mod surkaa_pad;

use crate::cache_file_manager::CacheFileManager;
use crate::encryption_manager::EncryptionManager;
use crate::oss_client_manager::OssClientManager;
use crate::secure_diary_store::{DiaryManifest, DownloadAttachmentEvent, SecureDiaryStore};
use crate::surkaa_pad::{AppState, DiaryMemoryCache};
use std::fs::{read, remove_file};
use std::ops::Deref;
use tauri::ipc::Channel;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_log::{log, Target, TargetKind};
use tauri_plugin_store::Builder;
//------------
// 解锁与加密解密 以及初始化云端存储客户端
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
/// * `akid` - 访问密钥 ID
/// * `aks` - 访问密钥 Secret
/// * `bucket` - 存储桶名称
/// * `endpoint` - OSS 端点
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn init_oss_client(
    client_state: State<'_, OssClientManager>,
    akid: &str,
    aks: &str,
    bucket: &str,
    endpoint: &str,
) -> Result<(), String> {
    client_state
        .initialize(akid, aks, endpoint, bucket)
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
async fn list_local_diaries(
    cache: State<'_, DiaryMemoryCache>,
    em: State<'_, EncryptionManager>,
    store: State<'_, SecureDiaryStore>,
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<Vec<DiaryManifest>, String> {
    app_state
        .load_cache_to_memory(cache.deref(), em.deref(), store.deref(), Some(&app_handle))
        .await?;
    let diaries = app_state.list_cached_diaries(cache.deref()).await;
    Ok(diaries)
}

/// 从 OSS 同步日记到本地缓存
/// # Arguments
/// * `uuid` - 可选的日记 UUID，若提供则只同步该日记，否则同步所有日记
/// # Returns
/// * `Result<Option<DiaryManifest>, String>` - 如果传入了 UUID，成功时返回该日记的清单，否则返回 None；失败时返回错误信息
#[tauri::command]
async fn sync_from_oss(
    cache: State<'_, DiaryMemoryCache>,
    em: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
    uuid: Option<String>,
) -> Result<Option<DiaryManifest>, String> {
    app_state
        .sync_from_oss(
            cache.deref(),
            em.deref(),
            client.deref(),
            store.deref(),
            Some(&app_handle),
            uuid,
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
    cache: State<'_, DiaryMemoryCache>,
    keyword: &str,
) -> Result<Vec<String>, String> {
    let map = cache.diaries.lock().await;
    let results: Vec<DiaryManifest> = map
        .values()
        .filter(|diary| diary.content.contains(keyword))
        .cloned()
        .collect();
    Ok(results.into_iter().map(|diary| diary.id).collect())
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
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
    content: &str,
) -> Result<DiaryManifest, String> {
    store
        .create_diary(
            encryption.deref(),
            client.deref(),
            app_state.deref(),
            Some(&app_handle),
            content,
        )
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
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
    uuid: String,
    new_content: &str,
) -> Result<DiaryManifest, String> {
    store
        .update_diary_content_only(
            encryption.deref(),
            client.deref(),
            app_state.deref(),
            Some(&app_handle),
            uuid,
            new_content,
        )
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
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
    uuid: String,
) -> Result<(), String> {
    store
        .delete_diary(client.deref(), app_state.deref(), Some(&app_handle), uuid)
        .await
}

//------------
// 附件操作相关
//------------

/// 添加附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `filename` - 临时附件文件名
/// * `minetype` - 附件 MIME 类型
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn add_attachment(
    encryption: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
    uuid: String,
    filename: String,
    minetype: String,
) -> Result<DiaryManifest, String> {
    // 获取临时文件的完整路径
    let temp_path = app_handle
        .path()
        .resolve(&filename, BaseDirectory::Temp)
        .map_err(|e| format!("无法解析临时文件路径: {}", e))?;

    // 在 Rust 中安全地读取大文件字节 (Vec<u8>)
    let bytes: Vec<u8> = read(&temp_path).map_err(|e| format!("无法读取临时文件: {}", e))?;

    // 删除缓存文件
    remove_file(temp_path).map_err(|e| format!("无法删除临时文件: {}", e))?;

    store
        .add_attachment(
            encryption.deref(),
            client.deref(),
            app_state.deref(),
            Some(&app_handle),
            uuid,
            bytes,
            minetype,
        )
        .await
}

/// 下载附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `filename` - 附件 ID
/// * `nonce` - 解密iv
/// # Returns
/// * `Result<Vec<u8>, String>` - 成功时返回附件字节数据，失败时返回错误信息
#[tauri::command]
fn download_attachment(
    encryption: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    app_state: State<'_, AppState>,
    cfm: State<'_, CacheFileManager>,
    app_handle: AppHandle,
    on_event: Channel<DownloadAttachmentEvent>,
    uuid: String,
    filename: String,
    nonce: Vec<u8>,
    eid: String,
) -> Result<(), String> {
    let attachment_cache = store
        .download_attachment(
            encryption.deref(),
            client.deref(),
            app_state.deref(),
            app_handle,
            on_event,
            uuid,
            filename,
            nonce,
            eid,
        )
        .map_err(|e| format!("下载附件失败: {}", e))?;

    cfm.add_cache_file(attachment_cache)
}

/// 取消下载附件 用于附件太大还未下载完成时 页面就退出了
/// # Arguments
/// * `eid` - 附件下载任务 ID
/// # Returns
/// * `Result<bool, String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
fn cancel_download_attachment(
    store: State<'_, SecureDiaryStore>,
    eid: &str,
) -> Result<bool, String> {
    store.cancel_download(eid)
}

/// 删除附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `filename` - 附件 ID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn delete_attachment(
    encryption: State<'_, EncryptionManager>,
    client: State<'_, OssClientManager>,
    store: State<'_, SecureDiaryStore>,
    app_state: State<'_, AppState>,
    app_handle: AppHandle,
    uuid: String,
    filename: String,
) -> Result<DiaryManifest, String> {
    store
        .delete_attachment(
            encryption.deref(),
            client.deref(),
            app_state.deref(),
            Some(&app_handle),
            uuid,
            filename,
        )
        .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        // 注册 Store 插件
        .plugin(Builder::default().build())
        // 注册文件系统插件
        .plugin(tauri_plugin_fs::init())
        // 注册日志插件
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
        .setup(|app| {
            app.manage(EncryptionManager::new());
            app.manage(OssClientManager::default());
            app.manage(SecureDiaryStore::default());
            app.manage(DiaryMemoryCache::new());
            app.manage(AppState::default());
            let cfm = CacheFileManager::new();
            app.manage(cfm.clone());

            let main_window = app.get_webview_window("main").unwrap();

            // 监听窗口关闭事件
            main_window.on_window_event(move |event| {
                match event {
                    WindowEvent::CloseRequested { .. } => {
                        // 在异步运行时中执行清理操作
                        let files = cfm.get_cache_files().unwrap_or_default();
                        log::info!(
                            "请求关闭窗口, 将开始清理缓存文件, 共计 {} 个文件",
                            files.len()
                        );
                        for file_path in files {
                            if !file_path.exists() {
                                log::warn!("缓存文件不存在，跳过删除: {:?}", file_path);
                                continue;
                            }

                            // 删除缓存文件
                            match remove_file(&file_path) {
                                Ok(_) => log::info!("已删除缓存文件: {:?}", file_path),
                                Err(e) => {
                                    log::error!("删除缓存文件时出错 {:?}: {}", file_path, e);
                                }
                            }
                        }
                    }
                    WindowEvent::Destroyed => {
                        // 窗口已销毁
                        log::info!("Window destroyed");
                    }
                    _ => {}
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            unlock,
            encrypt_data,
            decrypt_data,
            init_oss_client,
            list_local_diaries,
            sync_from_oss,
            search_diaries,
            save_diary,
            update_diary_content_only,
            delete_diary,
            add_attachment,
            download_attachment,
            cancel_download_attachment,
            delete_attachment
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
