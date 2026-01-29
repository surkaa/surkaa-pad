pub mod cache;

use crate::crypto::Crypto;
use crate::diary::cache::MemoryDiaryCache;
use crate::object::OssClient;
use crate::secure_diary_store::{
    diary_decrypt_bytes_to_manifest, diary_get_diary_manifest, diary_list_diaries, DiaryManifest,
};
use std::collections::{HashMap, HashSet};
use std::env::current_dir;
use std::fs::{create_dir_all, read_dir, write};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::log;

const CACHE_DIARY_DIR: &str = "diary_cache";
const CACHE_ATTACHMENT_DIR: &str = "attachment_cache";
const ATTACHMENT_EXTENSION: &str = ".enc";

/// 获取应用的日记缓存目录
pub fn pad_get_diary_cache_dir(app_handle: Option<&AppHandle>) -> PathBuf {
    let path = if let Some(app_handle) = app_handle {
        app_handle
            .path()
            .app_data_dir()
            .unwrap()
            .join(CACHE_DIARY_DIR)
    } else {
        let mut path = current_dir().expect("Failed to get current directory");
        path.push(CACHE_DIARY_DIR);

        path
    };

    if !path.exists() {
        create_dir_all(&path).expect("Failed to create diary cache directory");
    }
    path
}

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

/// 将本地文件加载到内存缓存中
pub async fn pad_load_cache_to_memory(
    dc: &MemoryDiaryCache,
    crypto: &Crypto,
    app_handle: Option<&AppHandle>,
) -> Result<(), String> {
    let cache_dir = pad_get_diary_cache_dir(app_handle);
    dc.clean();

    if !cache_dir.exists() {
        return Ok(());
    }

    // 遍历缓存目录下的所有 .enc 文件
    let entries =
        read_dir(cache_dir).map_err(|e| format!("Failed to read cache directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        // 确保只处理 .enc 文件
        if path.extension().and_then(|s| s.to_str()) == Some("enc") {
            // 1. 读取本地密文
            let encrypted_data = std::fs::read(&path)
                .map_err(|e| format!("Failed to read cached file {}: {}", path.display(), e))?;

            // 2. 解析文件名以获取 UUID (例如从 uuid_etag.enc 中获取 uuid)
            let filename = path.file_stem().unwrap().to_str().unwrap();
            let uuid = filename
                .rsplit_once('_')
                .map(|(uuid, _)| uuid)
                .unwrap_or(filename);

            // 3. 解密和反序列化
            if let Ok(manifest) = diary_decrypt_bytes_to_manifest(&crypto, &encrypted_data).await {
                // 4. 存入内存
                dc.insert(uuid, manifest);
            } else {
                // 记录错误，但继续处理其他文件
                eprintln!("Warning: Failed to decrypt cached file: {}", path.display());
            }
        }
    }

    Ok(())
}

/// 从 OSS 执行全量同步：清空本地缓存，下载所有 Manifest
pub async fn pad_sync_from_oss(
    dc: &MemoryDiaryCache,
    crypto: &Crypto,
    client: Arc<OssClient>,
    app_handle: Option<&AppHandle>,
    uuid: Option<String>,
) -> Result<Option<DiaryManifest>, String> {
    // 先加载内存的
    let cache_dir = pad_get_diary_cache_dir(app_handle);
    log::info!("加载日记缓存目录: {}", cache_dir.display());
    let local_entries = read_dir(&cache_dir).map_err(|e| format!("读取缓存目录失败: {}", e))?;
    // 把本地的缓存文件都读一遍，构建 local_uuid_for_etags
    let mut local_uuid_for_etags: HashMap<String, HashSet<String>> = HashMap::new();
    for entry in local_entries {
        let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
        let path = entry.path();

        // 如果uuid不为空，则跳过不匹配的文件
        if let Some(ref filter_uuid) = uuid {
            let filename = path.file_stem().unwrap().to_str().unwrap();
            let (file_uuid, _) = filename
                .rsplit_once('_')
                .ok_or_else(|| format!("无效的缓存文件名格式: {}", filename))?;
            if file_uuid != filter_uuid {
                continue;
            }
        }

        if path.extension().and_then(|s| s.to_str()) == Some("enc") {
            let filename = path.file_stem().unwrap().to_str().unwrap();
            let (uuid, etag) = filename
                .rsplit_once('_')
                .ok_or_else(|| format!("无效的缓存文件名格式: {}", filename))?;

            log::info!("发现本地缓存文件，UUID: {}, ETag: {}", uuid, etag);

            local_uuid_for_etags
                .entry(uuid.to_string())
                .or_insert_with(HashSet::new)
                .insert(etag.to_string());
        }
    }
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
    for (uuid, remote_etag) in remote_diaries_map.iter() {
        log::info!("处理远程日记 UUID: {}, ETag: {}", uuid, remote_etag);
        // 如果本地有对应一样的ETag，就跳过下载
        let local_etags = local_uuid_for_etags.get(uuid).cloned().unwrap_or_default();
        if !local_etags.contains(remote_etag) {
            log::info!("日记缓存未命中 {}. 准备下载.", uuid);

            // 下载和解密日记Manifest
            let (manifest, manifest_bytes) =
                diary_get_diary_manifest(&crypto, client.clone(), uuid.to_string()).await?;

            let new_filename = format!("{}_{}{}", uuid, remote_etag, ATTACHMENT_EXTENSION);
            let new_file_path = cache_dir.join(&new_filename);

            log::info!("准备写入缓存文件: {}", new_file_path.display());

            // 写入本地文件系统
            write(&new_file_path, &manifest_bytes).map_err(|e| {
                format!(
                    "未能写入缓存文件 new_filename: {}, Err: {}",
                    new_filename, e
                )
            })?;

            log::info!(
                "日记 {} 下载并缓存成功，文件路径: {}",
                uuid,
                new_file_path.display()
            );

            // 更新内存缓存
            dc.insert(uuid, manifest);
        } else {
            log::info!("日记缓存命中 {}. 跳过下载.", uuid);
        }
        // 删除本地多余的（uuid一样，etag却不一样）
        for etag in local_etags {
            if etag == *remote_etag {
                continue;
            }
            let obsolete_filename = format!("{}_{}{}", uuid, etag, ATTACHMENT_EXTENSION);
            let obsolete_file_path = cache_dir.join(&obsolete_filename);
            pad_remove_file(&obsolete_file_path).await?;
            log::info!("已删除的过时缓存文件: {}", obsolete_file_path.display());
        }
    }
    // 再删除本地多余的UUID
    let remote_uuids: HashSet<String> = remote_diaries_map.keys().cloned().collect();
    for (uuid, etags) in local_uuid_for_etags {
        if !remote_uuids.contains(&uuid) {
            // 这个UUID已经不在远程了，删除所有相关文件
            for etag in etags {
                let obsolete_filename = format!("{}_{}{}", uuid, etag, ATTACHMENT_EXTENSION);
                let obsolete_file_path = cache_dir.join(&obsolete_filename);
                pad_remove_file(&obsolete_file_path).await?;
                log::info!("已删除的过时缓存文件: {}", obsolete_file_path.display());
            }
        }
    }

    // 返回指定 UUID 的 Manifest（如果有的话）
    if let Some(filter_uuid) = uuid {
        Ok(dc.get(&filter_uuid))
    } else {
        Ok(None)
    }
}

async fn pad_remove_file(path: &PathBuf) -> Result<(), String> {
    if path.exists() {
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| format!("文件删除失败 {}: {}", path.display(), e))?;
    }
    Ok(())
}
