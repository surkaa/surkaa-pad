use crate::caches::LocalFileCache;
use crate::diaries::diary_store::{DiaryStore, LocalStore, RemoteStore};
use crate::diaries::DiaryError;
use crate::object::OssClient;
use crate::storages::{diary_id_from_manifest_key, remote_attachments_key};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri_plugin_log::log;

#[derive(Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum SyncProgressEvent {
    Started { total: u32 },
    Progress { current: u32, total: u32, diary_title: String },
    Completed,
    Error(String),
}

/// 将本地数据同步到云端（启用远程存储时调用）
pub async fn sync_local_to_cloud(
    lfc: &LocalFileCache,
    client: &OssClient,
    event: &Channel<SyncProgressEvent>,
) -> Result<(), DiaryError> {
    let local_store = LocalStore::new(lfc.clone());
    let remote_store = RemoteStore::new(lfc.clone(), client.clone());

    // 获取所有本地日记 ID
    let (ids, _) = local_store.list_diary_ids(None).await?;
    let total = ids.len() as u32;
    let _ = event.send(SyncProgressEvent::Started { total });

    for (i, id) in ids.iter().enumerate() {
        // 获取日记 manifest
        let (manifest_data, _etag) = local_store.download_manifest(id).await?;

        // 上传 manifest 到云端
        let new_etag = remote_store.upload_manifest(id, &manifest_data).await?;

        // 解析 manifest 获取标题（用于进度显示）
        let title = {
            let manifest_bytes = manifest_data.clone();
            // 尝试解密获取标题，失败则用 ID
            String::from_utf8_lossy(&manifest_bytes)
                .chars()
                .take(20)
                .collect::<String>()
        };

        let _ = event.send(SyncProgressEvent::Progress {
            current: i as u32 + 1,
            total,
            diary_title: title,
        });

        log::info!("[sync] uploaded manifest {}/{}: id={}, etag={}", i + 1, total, id, new_etag);
    }

    // 同步附件：遍历 LFC 中所有非 manifest 的文件
    let all_entries = lfc.get_all().await?;
    let attachment_entries: Vec<_> = all_entries
        .into_iter()
        .filter(|(key, _)| {
            // 过滤出附件文件（不是 manifest.enc 的文件）
            !key.ends_with("/manifest.enc") && key.contains('/')
        })
        .collect();

    for (key, local_md5) in &attachment_entries {
        // 上传附件到云端
        let data = lfc.get_data(key).await?;
        let remote_etag = client.upload_bytes(key, &data).await?;
        // 更新本地 MD5 为云端 etag
        lfc.save_bytes(key, &data).await?;

        log::info!("[sync] uploaded attachment: key={}, etag={}", key, remote_etag);
    }

    let _ = event.send(SyncProgressEvent::Completed);
    Ok(())
}

/// 将云端数据同步到本地（禁用远程存储时调用）
pub async fn sync_cloud_to_local(
    lfc: &LocalFileCache,
    client: &OssClient,
    event: &Channel<SyncProgressEvent>,
) -> Result<(), DiaryError> {
    let remote_store = RemoteStore::new(lfc.clone(), client.clone());

    // 分页列举所有云端日记
    let mut all_ids = Vec::new();
    let mut next_token = None;
    loop {
        let (ids, nt) = remote_store.list_diary_ids(next_token).await?;
        all_ids.extend(ids);
        if nt.is_none() {
            break;
        }
        next_token = nt;
    }

    let total = all_ids.len() as u32;
    let _ = event.send(SyncProgressEvent::Started { total });

    for (i, id) in all_ids.iter().enumerate() {
        // 下载 manifest 到本地
        let (manifest_data, etag) = remote_store.download_manifest(id).await?;
        // 确保本地缓存是最新的
        let key = crate::storages::remote_manifest_key(id);
        lfc.save_bytes(&key, &manifest_data).await?;

        let _ = event.send(SyncProgressEvent::Progress {
            current: i as u32 + 1,
            total,
            diary_title: id.clone(),
        });

        log::info!("[sync] downloaded manifest {}/{}: id={}, etag={}", i + 1, total, id, etag);
    }

    // 下载所有附件：遍历云端所有对象，过滤出附件
    let mut next_token = None;
    loop {
        let (objects, nt) = client.list("", next_token).await?;
        for obj in objects {
            // 跳过 manifest 文件
            if obj.key.ends_with("/manifest.enc") {
                continue;
            }
            // 检查本地是否已有且 etag 匹配
            if let Some(local_md5) = lfc.get(&obj.key).await? {
                if obj.etag.as_deref() == Some(&local_md5) {
                    continue; // 本地已是最新
                }
            }
            // 下载附件到本地
            let data = client.download_bytes(&obj.key).await?;
            lfc.save_bytes(&obj.key, &data).await?;
            log::info!("[sync] downloaded attachment: key={}", obj.key);
        }
        if nt.is_none() {
            break;
        }
        next_token = nt;
    }

    let _ = event.send(SyncProgressEvent::Completed);
    Ok(())
}
