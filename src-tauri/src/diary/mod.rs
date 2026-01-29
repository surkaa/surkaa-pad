pub mod cache;

use crate::crypto::Crypto;
use crate::diary::cache::MemoryDiaryCache;
use crate::object::OssClient;
use crate::secure_diary_store::{
    diary_get_diary_manifest, diary_list_diaries, DiaryManifest,
};
use std::collections::{HashMap};
use std::env::current_dir;
use std::fs::{create_dir_all};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::log;

const CACHE_ATTACHMENT_DIR: &str = "attachment_cache";
// const ATTACHMENT_EXTENSION: &str = ".enc";

/// 获取应用的附件缓存目录
pub fn pad_get_attachment_cache_dir(app_handle: Option<&AppHandle>) -> PathBuf {
    let path = if let Some(app_handle) = app_handle {
        app_handle
            .path()
            .app_data_dir()
            .unwrap()
            .join(CACHE_ATTACHMENT_DIR)
    } else {
        let mut path = current_dir().expect("Failed to get current directory");
        path.push(CACHE_ATTACHMENT_DIR);

        path
    };

    if !path.exists() {
        create_dir_all(&path).expect("Failed to create attachment cache directory");
    }
    path
}

/// 从 OSS 执行全量同步：清空本地缓存，下载所有 Manifest
pub async fn pad_sync_from_oss(
    dc: &MemoryDiaryCache,
    crypto: &Crypto,
    client: Arc<OssClient>,
    uuid: Option<String>,
) -> Result<Option<DiaryManifest>, String> {
    // 获取远程列表
    let remote_diaries_map: HashMap<String, String> = diary_list_diaries(client.clone(), &uuid)
        .await?
        .iter()
        .map(|(uuid, diary)| (uuid.clone(), diary.etag().to_string()))
        .collect();
    log::info!(
        "远程日记列表获取成功，共 {} 条日记",
        remote_diaries_map.len()
    );
    // 对比本地和远程的 UUID 和 ETag
    for (uuid, _remote_etag) in remote_diaries_map.iter() {
        // 下载和解密日记Manifest
        let (manifest, _) =
            diary_get_diary_manifest(&crypto, client.clone(), uuid.to_string()).await?;
        // 更新内存缓存
        dc.insert(uuid, manifest);
    }

    // 返回指定 UUID 的 Manifest（如果有的话）
    if let Some(filter_uuid) = uuid {
        Ok(dc.get(&filter_uuid))
    } else {
        Ok(None)
    }
}
