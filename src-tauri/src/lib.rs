mod attachment;
mod crypto;
mod diary;
mod object;
mod storage;
mod task;
mod utils;

use crate::attachment::attachment_protocol;
use crate::attachment::command::{cmd_add_attachment, cmd_delete_attachment};
use crate::crypto::command::{
    cmd_biometric_unlock, cmd_decrypt_data, cmd_encrypt_data, cmd_unlock,
};
use crate::crypto::Crypto;
use crate::diary::command::{
    cmd_delete_diary, cmd_get_diary_content, cmd_get_diary_summary, cmd_page_diary_ids,
    cmd_save_diary, cmd_search_diaries, cmd_update_diary_content_only,
};
use crate::diary::DiaryMemoryCache;
use crate::object::command::cmd_init_oss_client;
use crate::object::OssState;
use crate::task::command::cmd_cancel_task;
use crate::task::TaskPool;
use crate::utils::command::open_devtools;
use tauri::{App, Manager};

fn run_setup(app: &mut App) {
    app.manage(Crypto::new());
    app.manage(OssState::new());
    app.manage(TaskPool::new());
    app.manage(DiaryMemoryCache::new());

    #[cfg(target_os = "android")]
    let _ = app.handle().plugin(tauri_plugin_biometric::init());
}

fn generate_specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    let builder =
        tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
            // 解锁与加密解密
            cmd_unlock,
            cmd_encrypt_data,
            cmd_decrypt_data,
            cmd_biometric_unlock,
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
            cmd_delete_attachment,
            // 其他
            cmd_cancel_task,
            open_devtools,
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
        .register_asynchronous_uri_scheme_protocol("attachment", attachment_protocol)
        .setup(move |app| {
            run_setup(app);
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
