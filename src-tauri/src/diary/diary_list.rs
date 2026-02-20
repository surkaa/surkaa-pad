use crate::object::{NextToken, OssState};

/// 分页列出diary主键列表
/// # Arguments
/// * `next_token` - 分页的token
/// # Returns
/// * `Vec<String>` - diary主键列表
#[tauri::command]
#[specta::specta]
pub async fn list_diaries(
    client: tauri::State<'_, OssState>,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), String> {
    let client = client.get_client()?;
    let (objects, nt) = client
        .list("", next_token)
        .await
        .map_err(|e| format!("获取列表失败:{}", e))?;
    let keys = objects
        .into_iter()
        .map(|o| o.key().to_string())
        .collect();
    Ok((keys, nt))
}
