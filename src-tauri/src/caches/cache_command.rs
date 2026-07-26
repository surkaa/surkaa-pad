use crate::error::AppError;
use crate::object::Object;
use crate::state::AppState;
use std::collections::HashMap;
use tauri::State;

/// 清空所有本地对象数据
/// # Returns
/// * `Result<(), AppError>` - 成功时本地存储目录已清空
#[tauri::command]
#[specta::specta]
pub async fn cmd_clean_cache_file(state: State<'_, AppState>) -> Result<(), AppError> {
    Ok(state.local_file_cache().delete_all().await?)
}

/// 清理本地与 OSS 不一致或云端已不存在的缓存对象
/// # Returns
/// * `Result<Vec<String>, AppError>` - 已删除的本地对象 key 列表
#[tauri::command]
#[specta::specta]
pub async fn cmd_clean_unused_file(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    clean_unused_file(&state).await
}

async fn clean_unused_file(state: &AppState) -> Result<Vec<String>, AppError> {
    let _storage_mode_guard = state.lock_storage_operation().await;
    // 本地模式下 LFC 是权威存储，不能用云端对象列表作为删除依据。
    if !state.is_remote_enabled() {
        return Ok(Vec::new());
    }

    let client = state.oss_client();
    // 列出所有匹配的对象
    let mut next_token: Option<String> = None;
    let mut remote_objs = HashMap::new();
    loop {
        let (objects, nt) = client.list("", next_token).await?;
        for object in objects {
            remote_objs.insert(object.key.clone(), object);
        }
        if nt.is_none() {
            break;
        }
        next_token = nt;
    }
    let lfc = state.local_file_cache();
    let all_files = lfc.get_all().await?;
    let need_deletion = plan_stale_cache_keys(&all_files, &remote_objs);
    delete_cache_keys(&lfc, &need_deletion).await
}

fn plan_stale_cache_keys(
    local_files: &[(String, String)],
    remote_objects: &HashMap<String, Object>,
) -> Vec<String> {
    local_files
        .iter()
        .filter(|(key, etag)| {
            remote_objects
                .get(key)
                .and_then(|object| object.etag.as_deref())
                != Some(etag.as_str())
        })
        .map(|(key, _)| key.clone())
        .collect()
}

async fn delete_cache_keys(
    lfc: &crate::caches::LocalFileCache,
    keys: &[String],
) -> Result<Vec<String>, AppError> {
    let mut deleted = Vec::with_capacity(keys.len());
    for key in keys {
        lfc.delete(key).await?;
        deleted.push(key.clone());
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::LocalFileCache;
    use crate::cryptos::Crypto;
    use crate::object::OssClient;

    fn remote_object(key: &str, etag: Option<&str>) -> Object {
        Object {
            key: key.to_string(),
            size: 0,
            etag: etag.map(str::to_string),
        }
    }

    #[test]
    fn stale_cache_plan_keeps_only_matching_remote_etags() {
        let local_files = vec![
            ("matching".to_string(), "ETAG-1".to_string()),
            ("changed".to_string(), "OLD".to_string()),
            ("missing".to_string(), "ETAG-3".to_string()),
            ("remote-without-etag".to_string(), "ETAG-4".to_string()),
        ];
        let remote_objects = HashMap::from([
            (
                "matching".to_string(),
                remote_object("matching", Some("ETAG-1")),
            ),
            ("changed".to_string(), remote_object("changed", Some("NEW"))),
            (
                "remote-without-etag".to_string(),
                remote_object("remote-without-etag", None),
            ),
        ]);

        assert_eq!(
            plan_stale_cache_keys(&local_files, &remote_objects),
            vec!["changed", "missing", "remote-without-etag"]
        );
    }

    #[tokio::test]
    async fn local_mode_skips_cleanup_without_initialized_oss() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        lfc.save_bytes("local-only", b"must stay").await.unwrap();
        let state = AppState::from_parts(Crypto::new(), OssClient::new(), lfc.clone());

        let deleted = clean_unused_file(&state).await.unwrap();

        assert!(deleted.is_empty());
        assert_eq!(lfc.get_data("local-only").await.unwrap(), b"must stay");
    }

    #[tokio::test]
    async fn stale_cache_deletion_reports_only_successful_deletes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        lfc.save_bytes("keep", b"keep").await.unwrap();
        lfc.save_bytes("delete", b"delete").await.unwrap();

        let deleted = delete_cache_keys(&lfc, &["delete".to_string()])
            .await
            .unwrap();

        assert_eq!(deleted, vec!["delete"]);
        assert!(lfc.get("delete").await.unwrap().is_none());
        assert!(lfc.get("keep").await.unwrap().is_some());
    }
}
