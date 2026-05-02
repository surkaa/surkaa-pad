use crate::attachments::get_full_attachment_url;
use crate::caches::{DiaryMemoryCache, LocalFileCache};
use crate::cryptos::Crypto;
use crate::diaries::diary_types::DiarySummary;
use crate::diaries::{get_diary, DiaryError};
use crate::object::{NextToken, OssClient};
use crate::storages::diary_id_from_manifest_key;
use std::collections::HashMap;

pub async fn page_diary_ids(
    client: &OssClient,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), DiaryError> {
    let (objects, nt) = client.list("", next_token).await?;
    let mut ids: Vec<String> = Vec::with_capacity(objects.len());
    for obj in objects {
        if let Some(id) = diary_id_from_manifest_key(&obj.key) {
            ids.push(id);
        }
    }
    Ok((ids, nt))
}

pub async fn get_diary_summary(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
) -> Result<DiarySummary, DiaryError> {
    let diary = get_diary(cache, lfc, crypto, client, id).await?;
    Ok(DiarySummary::from_manifest(diary))
}

pub async fn get_diary_content(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
) -> Result<(String, HashMap<String, String>), DiaryError> {
    let diary = get_diary(cache, lfc, crypto, client, id).await?;
    let mut map = HashMap::new();
    for attachment in diary.attachments {
        let url = get_full_attachment_url(id, &attachment, client).await?;
        map.insert(attachment.filename, url);
    }
    Ok((diary.content, map))
}
