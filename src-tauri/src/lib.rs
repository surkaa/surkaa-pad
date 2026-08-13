pub mod ai;
mod app_config;
mod attachments;
mod caches;
mod cryptos;
mod diaries;
mod error;
mod local_storage;
mod object;
mod state;
mod storages;
mod stream;
mod tasks;
#[cfg(test)]
mod test_utils;
mod utils;

use crate::ai::ai_command::{cmd_list_ai_models, cmd_run_ai_agent};
use crate::app_config::{AppConfigStore, APP_CONFIG_FILENAME};
use crate::attachments::attachment_command::{
    cmd_add_attachment, cmd_add_attachment_memory, cmd_add_image_attachment_from_camera,
    cmd_caching_attachment, cmd_delete_attachment, cmd_rotate_image_attachment,
    cmd_save_decrypt_attachment, cmd_toggle_attachment_encryption, cmd_update_attachment_filename,
};
use crate::attachments::chunked_upload_command::{
    cmd_abort_chunked_upload, cmd_finish_chunked_upload, cmd_start_chunked_upload, cmd_upload_chunk,
};
use crate::attachments::{bind_attachment_server, start_attachment_server};
use crate::cryptos::crypto_command::{
    cmd_biometric_unlock, cmd_decrypt_data, cmd_encrypt_data, cmd_encrypt_info, cmd_unlock,
    cmd_valid_password,
};
use crate::diaries::diary_command::{
    cmd_delete_diary, cmd_get_diary_detail, cmd_get_diary_manifest, cmd_get_diary_summary,
    cmd_inspect_diary_versions, cmd_page_diary_ids, cmd_save_diary, cmd_search_diaries,
    cmd_update_diary_content_only,
};
use crate::local_storage::migration::{
    cmd_get_local_storage_info, cmd_get_local_storage_migration_status, cmd_migrate_local_storage,
    cmd_open_local_storage, cmd_plan_local_storage_migration,
};
use crate::object::object_command::{
    cmd_disable_remote_storage, cmd_enable_remote_storage, cmd_get_attachment_cache_info,
    cmd_get_storage_mode, cmd_init_oss_client, cmd_migrate_legacy_remote_enabled,
    cmd_plan_disable_remote_storage, cmd_restore_remote_storage, cmd_set_attachment_cache_limit,
};
use crate::state::AppState;
use crate::tasks::task_command::cmd_cancel_task;
use tauri::{App, Manager};

fn run_setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    tauri_plugin_log::log::info!("[startup] Rust setup started");
    let paths = app.handle().path();
    let app_config = AppConfigStore::load(paths.app_config_dir()?.join(APP_CONFIG_FILENAME))?;
    tauri_plugin_log::log::info!("[startup] app config loaded");
    let local_storage = local_storage::LocalStorageManager::new(
        app_config.clone(),
        paths.app_local_data_dir()?,
        paths.app_cache_dir()?,
    );
    let los_path = local_storage.startup_root();
    let (listener, attachment_server) = bind_attachment_server()?;
    tauri_plugin_log::log::info!("[startup] attachment server bound");
    let state = AppState::new(los_path, attachment_server, app_config, local_storage);
    start_attachment_server(listener, state.clone());
    app.manage(state);
    tauri_plugin_log::log::info!("[startup] Rust setup completed");

    Ok(())
}

fn generate_specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    let builder =
        tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
            // 解锁与加密解密
            cmd_unlock,
            cmd_encrypt_data,
            cmd_decrypt_data,
            cmd_valid_password,
            cmd_biometric_unlock,
            cmd_encrypt_info,
            // 客户端初始化
            cmd_init_oss_client,
            // 远程存储管理
            cmd_enable_remote_storage,
            cmd_plan_disable_remote_storage,
            cmd_disable_remote_storage,
            cmd_get_storage_mode,
            cmd_get_attachment_cache_info,
            cmd_set_attachment_cache_limit,
            cmd_migrate_legacy_remote_enabled,
            cmd_restore_remote_storage,
            // 本地存储位置管理
            cmd_get_local_storage_info,
            cmd_get_local_storage_migration_status,
            cmd_open_local_storage,
            cmd_plan_local_storage_migration,
            cmd_migrate_local_storage,
            // 日记基本操作
            cmd_save_diary,
            cmd_update_diary_content_only,
            cmd_delete_diary,
            // 日记列表操作
            cmd_page_diary_ids,
            cmd_get_diary_summary,
            cmd_get_diary_detail,
            cmd_get_diary_manifest,
            cmd_search_diaries,
            cmd_inspect_diary_versions,
            // 附件相关操作
            cmd_add_attachment,
            cmd_add_attachment_memory,
            cmd_delete_attachment,
            cmd_add_image_attachment_from_camera,
            cmd_toggle_attachment_encryption,
            cmd_rotate_image_attachment,
            cmd_caching_attachment,
            cmd_save_decrypt_attachment,
            cmd_update_attachment_filename,
            // 分片上传
            cmd_start_chunked_upload,
            cmd_upload_chunk,
            cmd_finish_chunked_upload,
            cmd_abort_chunked_upload,
            // AI Agent
            cmd_list_ai_models,
            cmd_run_ai_agent,
            // 其他
            cmd_cancel_task,
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

    let app_builder = tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        // 注册文件系统插件
        .plugin(tauri_plugin_fs::init())
        // 注册日志插件
        .plugin(
            tauri_plugin_log::Builder::new()
                // 应用业务日志保留 INFO；网络/TLS 依赖只保留真正需要诊断的警告，
                // 避免握手和重试状态机的 TRACE 淹没同步过程。
                .level(tauri_plugin_log::log::LevelFilter::Info)
                .level_for("rustls", tauri_plugin_log::log::LevelFilter::Warn)
                .level_for("reqwest", tauri_plugin_log::log::LevelFilter::Warn)
                .level_for("s3", tauri_plugin_log::log::LevelFilter::Warn)
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
        .plugin(tauri_plugin_dialog::init());

    // Android 插件必须在 Builder 阶段注册。若在 setup 中动态注册，插件初始化会在
    // 持有 Tauri 内部插件注册锁时等待 Android 主线程，而主线程加载首页又需要该锁，
    // 两者竞争时会形成死锁并导致启动白屏。
    #[cfg(target_os = "android")]
    let app_builder = app_builder
        .plugin(tauri_plugin_biometric::init())
        .plugin(tauri_plugin_native_camera::init());

    app_builder
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            run_setup(app)?;
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
