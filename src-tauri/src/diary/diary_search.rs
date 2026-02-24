use crate::crypto::Crypto;
use crate::diary::diary_list::page_diary_ids;
use crate::diary::types::{DiarySummary, SearchDiariesEvent};
use crate::diary::{get_diary, DiaryMemoryCache};
use crate::object::{NextToken, OssClient};
use crate::utils::message_sender::MessageSender;
use std::sync::Arc;

pub async fn search_diaries(
    cache: &DiaryMemoryCache,
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
                    let diary = get_diary(cache, crypto, client, &id).await?;

                    let content = diary.content();
                    // 如果 or 是 true，则满足任一关键词即可；如果 or 是 false，则必须满足所有关键词
                    let is_match = if or {
                        kc.iter().any(|keyword| content.contains(keyword))
                    } else {
                        kc.iter().all(|keyword| content.contains(keyword))
                    };

                    ecc.send(if is_match {
                        SearchDiariesEvent::Match(DiarySummary::from_manifest(diary))
                    } else {
                        SearchDiariesEvent::Unmatch(diary.id)
                    })?;

                    Ok::<(), String>(())
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

        Ok::<(), String>(())
    };

    if let Err(e) = logic.await {
        let _ = ec.send(SearchDiariesEvent::Error(e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diary::diary::{delete_diary, save_diary};
    use serial_test::serial;

    async fn test_search(
        cache: &DiaryMemoryCache,
        crypto: &Crypto,
        client: &OssClient,
        keyword: String,
        or: bool,
    ) -> (Vec<DiarySummary>, Vec<String>) {
        // 创建事件监听器
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SearchDiariesEvent>();
        let event_sender = Arc::new(tx);
        let _ = search_diaries(&cache, &crypto, &client, event_sender.clone(), keyword, or).await;

        let mut matches = Vec::new();
        let mut unmatches = Vec::new();
        let mut finished = false;
        let mut error = None;

        while let Some(event) = rx.recv().await {
            match event {
                SearchDiariesEvent::Match(summary) => matches.push(summary),
                SearchDiariesEvent::Unmatch(id) => unmatches.push(id),
                SearchDiariesEvent::Finished => {
                    finished = true;
                    break;
                }
                SearchDiariesEvent::Error(e) => {
                    error = Some(e);
                    break;
                }
            }
        }

        assert!(error.is_none(), "搜索过程中发生错误: {:?}", error);
        assert!(finished, "搜索未正常完成");

        (matches, unmatches)
    }

    #[serial]
    #[tokio::test]
    async fn test_diary_search() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let cache = DiaryMemoryCache::new();

        // 确保是空的测试环境
        let (ids, _) = page_diary_ids(&client, None)
            .await
            .expect("无法获取日记列表");
        assert!(ids.is_empty(), "测试环境不干净，存在日记数据");

        // 创建几个测试日记
        let _ = save_diary(&crypto, &client, "这是第一篇日记，包含关键词 rust").await;
        let _ = save_diary(&crypto, &client, "这是第二篇日记，不包含关键词").await;
        let _ = save_diary(&crypto, &client, "这是第三篇日记，包含关键词 rust 和 async").await;
        let _ = save_diary(&crypto, &client, "这是第四篇日记，包含关键词 async").await;

        // 收集结果
        let (matches, unmatches) =
            test_search(&cache, &crypto, &client, "rust".to_string(), true).await;
        assert_eq!(matches.len(), 2, "使用 OR 搜索 'rust' 应该匹配 2 篇日记");
        assert_eq!(
            unmatches.len(),
            2,
            "使用 OR 搜索 'rust' 应该不匹配 2 篇日记"
        );

        let (matches, unmatches) =
            test_search(&cache, &crypto, &client, "async".to_string(), true).await;
        assert_eq!(matches.len(), 2, "使用 OR 搜索 'async' 应该匹配 2 篇日记");
        assert_eq!(
            unmatches.len(),
            2,
            "使用 OR 搜索 'async' 应该不匹配 2 篇日记"
        );

        let (matches, unmatches) =
            test_search(&cache, &crypto, &client, "rust async".to_string(), false).await;
        assert_eq!(
            matches.len(),
            1,
            "使用 AND 搜索 'rust async' 应该匹配 1 篇日记"
        );
        assert_eq!(
            unmatches.len(),
            3,
            "使用 AND 搜索 'rust async' 应该不匹配 3 篇日记"
        );

        let (matches, unmatches) =
            test_search(&cache, &crypto, &client, "rust async".to_string(), true).await;
        assert_eq!(
            matches.len(),
            3,
            "使用 OR 搜索 'rust async' 应该匹配 3 篇日记"
        );
        assert_eq!(
            unmatches.len(),
            1,
            "使用 OR 搜索 'rust async' 应该不匹配 1 篇日记"
        );

        // 清理测试数据
        let (ids, _) = page_diary_ids(&client, None)
            .await
            .expect("无法获取日记列表");
        for id in ids {
            let _ = delete_diary(&cache, &client, &id).await;
        }
    }
}
