mod attachments;
mod caches;
mod cryptos;
mod diaries;
mod object;
mod state;
mod storages;
mod stream;
mod tasks;
mod utils;

use crate::attachments::attachment_command::{
    cmd_add_attachment, cmd_add_attachment_memory, cmd_add_image_attachment_from_camera,
    cmd_delete_attachment, cmd_rotate_image_attachment, cmd_toggle_attachment_encryption,
    cmd_caching_attachment, cmd_save_decrypt_attachment
};
use crate::attachments::{attachment_protocol, PROTOCOL_NAME};
use crate::caches::cache_command::{cmd_clean_cache_file, cmd_clean_unused_file};
use crate::caches::LOCAL_FILE_CACHE_FILENAME;
use crate::cryptos::crypto_command::{
    cmd_biometric_unlock, cmd_decrypt_data, cmd_encrypt_data, cmd_encrypt_info, cmd_unlock,
};
use crate::diaries::diary_command::{
    cmd_delete_diary, cmd_get_diary_content, cmd_get_diary_summary, cmd_page_diary_ids,
    cmd_save_diary, cmd_search_diaries, cmd_update_diary_content_only,
};
use crate::object::object_command::cmd_init_oss_client;
use crate::state::AppState;
use crate::tasks::task_command::cmd_cancel_task;
use tauri::{App, Manager};

fn run_setup(app: &mut App) {
    let cache_path = app
        .handle()
        .path()
        .app_cache_dir()
        .expect("failed to get cache dir");
    let lfc_path = cache_path.join(LOCAL_FILE_CACHE_FILENAME);
    app.manage(AppState::new(lfc_path));

    #[cfg(target_os = "android")]
    {
        let _ = app.handle().plugin(tauri_plugin_biometric::init());
        let _ = app.handle().plugin(tauri_plugin_native_camera::init());
    }
}

fn generate_specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    let builder =
        tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
            // 解锁与加密解密
            cmd_unlock,
            cmd_encrypt_data,
            cmd_decrypt_data,
            cmd_biometric_unlock,
            cmd_encrypt_info,
            // 客户端初始化
            cmd_init_oss_client,
            // 日记基本操作
            cmd_save_diary,
            cmd_update_diary_content_only,
            cmd_delete_diary,
            // 日记列表操作
            cmd_page_diary_ids,
            cmd_get_diary_summary,
            cmd_get_diary_content,
            cmd_search_diaries,
            // 附件相关操作
            cmd_add_attachment,
            cmd_add_attachment_memory,
            cmd_delete_attachment,
            cmd_add_image_attachment_from_camera,
            cmd_toggle_attachment_encryption,
            cmd_rotate_image_attachment,
            cmd_caching_attachment,
            cmd_save_decrypt_attachment,
            // 其他
            cmd_cancel_task,
            cmd_clean_cache_file,
            cmd_clean_unused_file,
        ]);

    #[cfg(debug_assertions)]
    #[cfg(windows)]
    builder
        .export(
            specta_typescript::Typescript::default()
                .header("// @ts-nocheck\n/* eslint-disable */\n"),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

    builder
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = generate_specta_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
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
        .register_asynchronous_uri_scheme_protocol(PROTOCOL_NAME, attachment_protocol)
        .setup(move |app| {
            run_setup(app);
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
