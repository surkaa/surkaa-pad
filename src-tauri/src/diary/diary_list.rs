use crate::object::{NextToken, OssClient};

pub(super) async fn page_diary_ids(
    client: &OssClient,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), String> {
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
