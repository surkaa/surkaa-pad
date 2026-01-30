mod attachment;
mod crypto;
mod diary;
mod object;
mod storage;
mod task;
mod utils;

use std::ops::Deref;
use crate::attachment::DownloadAttachmentEvent;
use crate::attachment::{attachment_delete, attachment_download, attachment_upload};
use crate::diary::DiaryManifest;
use crate::diary::DiaryMemoryCache;
use crate::diary::{diary_create, diary_delete, diary_sync, diary_update_diary_content_only};
use crate::object::OssState;
use crate::storage::{local_attachment_dir, local_recording_dir};
use crate::task::TaskPool;
use crate::utils::open_file_stream;
use crypto::Crypto;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
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
    cache: State<'_, DiaryMemoryCache>,
) -> Result<Vec<DiaryManifest>, String> {
    Ok(cache.list())
}

/// 从 OSS 同步日记到本地缓存
/// # Arguments
/// * `uuid` - 可选的日记 UUID，若提供则只同步该日记，否则同步所有日记
/// # Returns
/// * `Result<Option<DiaryManifest>, String>` - 如果传入了 UUID，成功时返回该日记的清单，否则返回 None；失败时返回错误信息
#[tauri::command]
async fn sync_from_oss(
    cache: State<'_, DiaryMemoryCache>,
    em: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: Option<String>,
) -> Result<Option<DiaryManifest>, String> {
    diary_sync(cache.deref(), em.deref(), client.get_client()?, uuid).await
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
    content: &str,
) -> Result<DiaryManifest, String> {
    diary_create(crypto.deref(), client.get_client()?, content).await
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
    uuid: String,
    new_content: &str,
) -> Result<DiaryManifest, String> {
    diary_update_diary_content_only(crypto.deref(), client.get_client()?, uuid, new_content).await
}

/// 删除日记
/// # Arguments
/// * `uuid` - 日记 UUID
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn delete_diary(client: State<'_, OssState>, uuid: String) -> Result<(), String> {
    diary_delete(client.get_client()?, uuid).await
}

//------------
// 附件操作相关
//------------

/// 添加附件
/// # Arguments
/// * `uuid` - 日记 UUID
/// * `access_str` - 文件访问路径。
/// * `minetype` - 附件 MIME 类型
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
async fn add_attachment(
    crypto: State<'_, Crypto>,
    client: State<'_, OssState>,
    uuid: String,
    access_str: String,
    minetype: String,
) -> Result<DiaryManifest, String> {
    // 获取临时文件的完整路径
    let (len, stream) = open_file_stream(&access_str)?;

    attachment_upload(
        crypto.deref(),
        client.get_client()?,
        uuid,
        minetype,
        len,
        stream,
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
    app_handle: AppHandle,
    on_event: Channel<DownloadAttachmentEvent>,
    uuid: String,
    filename: String,
    nonce: Vec<u8>,
) -> Result<String, String> {
    let crypto = crypto.inner().clone();
    let client = client.get_client()?;
    tp.spawn(async move {
        attachment_download(
            Arc::new(crypto),
            client,
            &app_handle,
            Arc::new(on_event),
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
    uuid: String,
    filename: String,
) -> Result<DiaryManifest, String> {
    attachment_delete(crypto.deref(), client.get_client()?, uuid, filename).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 注册 Store 插件
        .plugin(tauri_plugin_store::Builder::default().build())
        // 注册文件系统插件
        .plugin(tauri_plugin_fs::init())
        // 注册日志插件
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: None }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .build(),
        )
        // 注册 os 插件
        .plugin(tauri_plugin_os::init())
        // 注册 dialog 插件
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Crypto::new());
            app.manage(OssState::new());
            app.manage(TaskPool::new());
            app.manage(DiaryMemoryCache::new());

            let app_handle = app.handle();

            #[cfg(mobile)]
            let _ = app_handle.plugin(tauri_plugin_biometric::init());

            let main_window = app.get_webview_window("main").expect("无法获取主窗口");

            let app_handle = app_handle.clone();
            main_window.on_window_event(move |event| match event {
                tauri::WindowEvent::CloseRequested { .. } => {
                    // 删除附件文件夹
                    let attachment_dir = local_attachment_dir(&app_handle);
                    if attachment_dir.exists() {
                        let _ = std::fs::remove_dir_all(&attachment_dir);
                    }
                    // 删除录音文件夹
                    let recording_dir = utils::local_recording_dir(&app_handle);
                    if recording_dir.exists() {
                        let _ = std::fs::remove_dir_all(&recording_dir);
                    }
                }
                _ => {}
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
