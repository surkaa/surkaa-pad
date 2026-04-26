use crate::caches::{DiaryMemoryCache, LocalFileCache};
use crate::cryptos::Crypto;
use crate::diaries::diary_list::page_diary_ids;
use crate::diaries::diary_types::{DiarySummary, SearchDiariesEvent};
use crate::diaries::{get_diary, DiaryError};
use crate::object::{NextToken, OssClient};
use crate::utils::message_sender::MessageSender;
use std::sync::Arc;

pub async fn search_diaries(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    event: Arc<dyn MessageSender<SearchDiariesEvent>>,
    keyword: String,
    or: bool,
) {
    let ec = event.clone();
    let keywords = keyword.split_whitespace().collect::<Vec<_>>();
    let logic = async move {
        let mut nt: NextToken = None;
        loop {
            let ec = event.clone();
            let (ids, next_token) = page_diary_ids(client, nt.clone()).await?;

            // 多线程搜索
            let fetches = ids.into_iter().map(|id| {
                let ecc = event.clone();
                let kc = keywords.clone();
                async move {
                    let diary = get_diary(cache, lfc, crypto, client, &id).await?;

                    let content = diary.content();
                    // 如果 or 是 true，则满足任一关键词即可；如果 or 是 false，则必须满足所有关键词
                    let is_match = if or {
                        kc.iter().any(|keyword| content.contains(keyword))
                    } else {
                        kc.iter().all(|keyword| content.contains(keyword))
                    };

                    let _ = ecc.send(if is_match {
                        SearchDiariesEvent::Match(DiarySummary::from_manifest(diary))
                    } else {
                        SearchDiariesEvent::Unmatch(diary.id)
                    });

                    Ok::<(), DiaryError>(())
                }
            });

            for res in futures_util::future::join_all(fetches).await {
                res?;
            }

            if next_token.is_none() {
                let _ = ec.send(SearchDiariesEvent::Finished);
                break;
            }
            nt = next_token;
        }

        Ok::<(), DiaryError>(())
    };

    if let Err(e) = logic.await {
        let _ = ec.send(SearchDiariesEvent::Error(e.to_string()));
    }
}
