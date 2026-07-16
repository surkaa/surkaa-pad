use crate::caches::DiaryMemoryCache;
use crate::cryptos::Crypto;
use crate::diaries::diary_store::DiaryStore;
use crate::diaries::diary_types::DiarySummary;
use crate::diaries::{get_diary, DiaryContent, DiaryError};
use crate::object::NextToken;
use std::collections::HashMap;

pub async fn page_diary_ids(
    store: &dyn DiaryStore,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), DiaryError> {
    store.list_diary_ids(next_token).await
}

pub async fn get_diary_summary(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    id: &str,
) -> Result<DiarySummary, DiaryError> {
    let diary = get_diary(cache, crypto, store, id).await?;
    Ok(DiarySummary::from_manifest(diary))
}

pub async fn get_diary_content(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    id: &str,
) -> Result<(DiaryContent, HashMap<String, String>), DiaryError> {
    let diary = get_diary(cache, crypto, store, id).await?;
    let mut map = HashMap::new();
    for attachment in diary.attachments {
        let url = store.get_attachment_url(id, &attachment).await?;
        map.insert(attachment.filename, url);
    }
    Ok((diary.content, map))
}
