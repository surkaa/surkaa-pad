mod attachment;
mod crypto;
mod diary;
mod object;
mod storage;
mod task;
mod utils;

use crate::attachment::{add_attachment, delete_attachment, download_attachment};
use crate::crypto::{biometric_unlock, decrypt_data, encrypt_data, unlock, Crypto};
use crate::diary::{
    delete_diary, list_diaries, save_diary, update_diary_content_only, DiaryMemoryCache,
};
use crate::object::{init_oss_client, OssState};
use crate::storage::{local_attachment_dir, local_recording_dir};
use crate::task::{cancel_task, TaskPool};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder =
        tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
            // 解锁与加密解密
            unlock,
            encrypt_data,
            decrypt_data,
            biometric_unlock,
            // 客户端初始化
            init_oss_client,
            // 日记基本操作
            save_diary,
            update_diary_content_only,
            delete_diary,
            // 日记列表操作
            list_diaries,
            // 附件相关操作
            add_attachment,
            download_attachment,
            delete_attachment,
            // 其他
            cancel_task,
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
