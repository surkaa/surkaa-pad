mod attachment;
mod crypto;
mod diary;
mod object;
mod storage;
mod task;
mod utils;

use crate::attachment::{add_attachment, delete_attachment, download_attachment};
use crate::crypto::Crypto;
use crate::diary::DiaryMemoryCache;
use crate::diary::{delete_diary, save_diary, update_diary_content_only};
use crate::object::OssState;
use crate::storage::{local_attachment_dir, local_recording_dir};
use crate::task::{cancel_task, TaskPool};
use tauri::{Manager, State};
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
#[specta::specta]
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
#[specta::specta]
async fn biometric_unlock(crypto: State<'_, Crypto>, dek: String) -> Result<(), String> {
    crypto.init_by_dek_string(dek)
}

/// 加密数据
/// # Arguments
/// * `data` - 待加密的数据
/// # Returns
/// * `Result<(Vec<u8>, Vec<u8>), String>` - 成功时返回 (密文, nonce)，失败时返回错误信息
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder =
        tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
            unlock,
            encrypt_data,
            decrypt_data,
            init_oss_client,
            save_diary,
            update_diary_content_only,
            delete_diary,
            add_attachment,
            download_attachment,
            cancel_task,
            delete_attachment,
            biometric_unlock
        ]);

    #[cfg(debug_assertions)]
    #[cfg(windows)]
    builder
        .export(
            specta_typescript::Typescript::default(),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

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
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: None,
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .build(),
        )
        // 注册 os 插件
        .plugin(tauri_plugin_os::init())
        // 注册 dialog 插件
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
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
                    let recording_dir = local_recording_dir(&app_handle);
                    if recording_dir.exists() {
                        let _ = std::fs::remove_dir_all(&recording_dir);
                    }
                }
                _ => {}
            });
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
