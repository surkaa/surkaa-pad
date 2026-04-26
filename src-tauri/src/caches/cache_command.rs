use std::collections::{HashMap};
use crate::error::AppError;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn cmd_clean_cache_file(state: State<'_, AppState>) -> Result<(), AppError> {
    Ok(state.local_file_cache().delete_all().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_clean_unused_file(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    let client = state.get_client()?;
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
    let all_files = state.local_file_cache().get_all().await?;
    let mut need_deletion = Vec::new();
    for (key, md5) in all_files {
        if let Some(obj) = remote_objs.get(&key) {
            if obj.etag.as_deref() != Some(&md5) {
                need_deletion.push(key);
            }
        } else {
            need_deletion.push(key);
        }
    }
    for key in &need_deletion {
        state.local_file_cache().delete(key).await;
    }
    Ok(need_deletion)
}
