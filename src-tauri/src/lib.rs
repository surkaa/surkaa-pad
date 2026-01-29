pub mod cache_file_manager;
mod crypto;
mod diary;
mod object;
pub mod secure_diary_store;
mod task;

use crate::cache_file_manager::CacheFileManager;
use crate::diary::cache::MemoryDiaryCache;
use crate::diary::{pad_load_cache_to_memory, pad_sync_from_oss};
use crate::object::OssState;
use crate::secure_diary_store::{
    diary_add_attachment, diary_create_diary, diary_delete_attachment, diary_delete_diary,
    diary_download_attachment, diary_update_diary_content_only, DiaryManifest,
    DownloadAttachmentEvent,
};
use crate::task::TaskPool;
use crypto::Crypto;
use std::fs::{read, remove_file};
use std::ops::Deref;
use std::sync::Arc;
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
    crypto: State<'_, Crypto>,
    master_password: String,
    salt: String,
) -> Result<String, String> {
    crypto.derive_dek(master_password, salt)
}

/// 生物解锁，传入dek解锁
/// # Arguments
/// * `dek` - 数据加密密钥
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn biometric_unlock(crypto: State<'_, Crypto>, dek: String) -> Result<(), String> {
    crypto.init_by_dek_string(dek)
}

/// 加密数据
/// # Arguments
/// * `data` - 待加密的数据
/// # Returns
/// * `Result<(Vec<u8>, Vec<u8>), String>` - 成功时返回 (密文, nonce)，失败时返回错误信息
#[tauri::command]
async fn encrypt_data(
    crypto: State<'_, Crypto>,
    data: String,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    crypto.encrypt(&data.as_bytes())
}

/// 解密数据
/// # Arguments
/// * `ciphertext` - 密文
/// * `nonce` - 解密用的 nonce
/// # Returns
/// * `Result<Vec<u8>, String>` - 成功时返回明文，失败时返回错误信息
#[tauri::command]
async fn decrypt_data(
    crypto: State<'_, Crypto>,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
) -> Result<String, String> {
    let decrypted_bytes = crypto.decrypt(&ciphertext, &nonce)?;
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
    client_state: State<'_, OssState>,
    akid: String,
    aks: String,
    bucket: String,
    endpoint: String,
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
    cache: State<'_, MemoryDiaryCache>,
    em: State<'_, Crypto>,
    app_handle: AppHandle,
) -> Result<Vec<DiaryManifest>, String> {
    pad_load_cache_to_memory(cache.deref(), em.deref(), Some(&app_handle)).await?;
    Ok(cache.list())
}

/// 从 OSS 同步日记到本地缓存
/// # Arguments
/// * `uuid` - 可选的日记 UUID，若提供则只同步该日记，否则同步所有日记
/// # Returns
/// * `Result<Option<DiaryManifest>, String>` - 如果传入了 UUID，成功时返回该日记的清单，否则返回 None；失败时返回错误信息
#[tauri::command]
async fn sync_from_oss(
    cache: State<'_, MemoryDiaryCache>,
    em: State<'_, Crypto>,
    client: State<'_, OssState>,
    app_handle: AppHandle,
    uuid: Option<String>,
) -> Result<Option<DiaryManifest>, String> {
    pad_sync_from_oss(
        cache.deref(),
        em.deref(),
        client.get_client()?,
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
    cache: State<'_, MemoryDiaryCache>,
    keyword: &str,
) -> Result<Vec<String>, String> {
    let diaries = cache.list();
    let results: Vec<DiaryManifest> = diaries
        .into_iter()
        .filter(|diary| diary.content.contains(keyword))
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
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    app_handle: AppHandle,
    content: &str,
) -> Result<DiaryManifest, String> {
    diary_create_diary(
        crypto.deref(),
        client.get_client()?,
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
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    app_handle: AppHandle,
    uuid: String,
    new_content: &str,
) -> Result<DiaryManifest, String> {
    diary_update_diary_content_only(
        crypto.deref(),
        client.get_client()?,
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
    client: State<'_, OssState>,
    app_handle: AppHandle,
    uuid: String,
) -> Result<(), String> {
    diary_delete_diary(client.get_client()?, Some(&app_handle), uuid).await
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
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
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

    diary_add_attachment(
        crypto.deref(),
        client.get_client()?,
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
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    tp: State<'_, TaskPool>,
    cfm: State<'_, CacheFileManager>,
    app_handle: AppHandle,
    on_event: Channel<DownloadAttachmentEvent>,
    uuid: String,
    filename: String,
    nonce: Vec<u8>,
) -> Result<String, String> {
    let crypto = crypto.inner().clone();
    let client = client.get_client()?.clone();
    let cfm = cfm.inner().clone();
    let app_handle = app_handle.clone();
    let on_event = on_event.clone();
    tp.spawn(async move {
        diary_download_attachment(
            Arc::new(crypto),
            client,
            Arc::new(cfm),
            app_handle,
            on_event,
            uuid,
            filename,
            nonce,
        )
        .await;
    })
}

/// 取消任务
/// # Arguments
/// * `cancel_token` - 任务取消令牌
/// # Returns
/// * `Result<bool, String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
fn cancel_task(tp: State<'_, TaskPool>, cancel_token: &str) -> Result<bool, String> {
    tp.cancel(cancel_token)
}

/// 删除附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `filename` - 附件 ID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn delete_attachment(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    app_handle: AppHandle,
    uuid: String,
    filename: String,
) -> Result<DiaryManifest, String> {
    diary_delete_attachment(
        crypto.deref(),
        client.get_client()?,
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
        // 注册 os 插件
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            app.manage(Crypto::new());
            app.manage(OssState::new());
            app.manage(TaskPool::new());
            app.manage(MemoryDiaryCache::new());
            let cfm = CacheFileManager::new();
            app.manage(cfm.clone());

            #[cfg(mobile)]
            let _ = app.handle().plugin(tauri_plugin_biometric::init());

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
            cancel_task,
            delete_attachment,
            biometric_unlock
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
