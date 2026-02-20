use crate::crypto::Crypto;
use crate::diary::types::DiarySummary;
use crate::diary::{diary_get, DiaryMemoryCache};
use crate::object::{NextToken, OssClient};

pub(super) async fn page_diary_ids(
    client: &OssClient,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), String> {
    let (objects, nt) = client
        .list("", next_token)
        .await
        .map_err(|e| format!("获取列表失败:{}", e))?;
    let keys = objects.into_iter().map(|o| o.key().to_string()).collect();
    Ok((keys, nt))
}

pub(super) async fn get_diary_summary(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: String,
) -> Result<DiarySummary, String> {
    let diary = diary_get(cache, crypto, client, id).await?;
    let title = diary
        .content
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let mut attachment_map = std::collections::HashMap::new();
    for att in diary.attachments {
        for prefix in ["IMG", "AUD", "VID"] {
            let mark = format!("<<{}:{}>>", prefix, att.filename);
            if diary.content.contains(&mark) {
                attachment_map.insert(prefix.to_string(), att.clone());
                break;
            }
        }
    }

    Ok(DiarySummary {
        id: diary.id,
        created: diary.created,
        updated: diary.updated,
        title,
        attachment_map,
    })
}

pub(super) async fn get_diary_content(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: String,
) -> Result<String, String> {
    let diary = diary_get(cache, crypto, client, id).await?;
    Ok(diary.content)
}
