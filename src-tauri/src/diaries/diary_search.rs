use crate::caches::DiaryMemoryCache;
use crate::cryptos::Crypto;
use crate::diaries::diary_content::{DiaryContent, DiaryContentNode};
use crate::diaries::diary_list::page_diary_ids;
use crate::diaries::diary_store::DiaryStore;
use crate::diaries::diary_types::{AttachmentTypeFilter, DiarySummary, SearchDiariesEvent};
use crate::diaries::{get_diary, DiaryError};
use crate::object::NextToken;
use crate::utils::message_sender::MessageSender;
use std::sync::Arc;

pub struct SearchDiaryQuery {
    pub keyword: String,
    pub keyword_or: bool,
    pub attachment_types: Vec<AttachmentTypeFilter>,
    pub attachment_or: bool,
}

pub async fn search_diaries(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    event: Arc<dyn MessageSender<SearchDiariesEvent>>,
    query: SearchDiaryQuery,
) {
    let ec = event.clone();
    let SearchDiaryQuery {
        keyword,
        keyword_or,
        attachment_types,
        attachment_or,
    } = query;
    let keywords = keyword
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let logic = async move {
        let mut nt: NextToken = None;
        loop {
            let ec = event.clone();
            let (ids, next_token) = page_diary_ids(store, nt.clone()).await?;

            // 多线程搜索
            let fetches = ids.into_iter().map(|id| {
                let ecc = event.clone();
                let kc = keywords.clone();
                let attachment_types = attachment_types.clone();
                async move {
                    let diary = get_diary(cache, crypto, store, &id).await?;

                    let searchable_text = diary.content.searchable_text();
                    // 如果 or 是 true，则满足任一关键词即可；如果 or 是 false，则必须满足所有关键词
                    let keyword_matches = if kc.is_empty() {
                        true
                    } else if keyword_or {
                        kc.iter().any(|keyword| searchable_text.contains(keyword))
                    } else {
                        kc.iter().all(|keyword| searchable_text.contains(keyword))
                    };
                    let attachment_matches = attachment_types.is_empty()
                        || if attachment_or {
                            attachment_types.iter().any(|filter| {
                                content_matches_attachment_filter(&diary.content, *filter)
                            })
                        } else {
                            attachment_types.iter().all(|filter| {
                                content_matches_attachment_filter(&diary.content, *filter)
                            })
                        };
                    let is_match = keyword_matches && attachment_matches;

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

fn content_matches_attachment_filter(content: &DiaryContent, filter: AttachmentTypeFilter) -> bool {
    content.nodes.iter().any(|node| match filter {
        AttachmentTypeFilter::Image => matches!(
            node,
            DiaryContentNode::Image { .. } | DiaryContentNode::Album { .. }
        ),
        AttachmentTypeFilter::Audio => matches!(node, DiaryContentNode::Audio { .. }),
        AttachmentTypeFilter::Video => matches!(node, DiaryContentNode::Video { .. }),
        AttachmentTypeFilter::Other => matches!(node, DiaryContentNode::File { .. }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diaries::diary_content::{AlbumDisplayMode, ImageSize};

    #[test]
    fn attachment_filter_classifies_content_nodes() {
        let content = DiaryContent {
            nodes: vec![
                DiaryContentNode::Image {
                    attachment_id: "image".to_string(),
                    size: ImageSize::Normal,
                },
                DiaryContentNode::Audio {
                    attachment_id: "audio".to_string(),
                },
                DiaryContentNode::Video {
                    attachment_id: "video".to_string(),
                },
                DiaryContentNode::File {
                    attachment_id: "file".to_string(),
                },
            ],
        };

        for filter in [
            AttachmentTypeFilter::Image,
            AttachmentTypeFilter::Audio,
            AttachmentTypeFilter::Video,
            AttachmentTypeFilter::Other,
        ] {
            assert!(content_matches_attachment_filter(&content, filter));
        }
    }

    #[test]
    fn attachment_filter_treats_album_as_image() {
        let content = DiaryContent {
            nodes: vec![DiaryContentNode::Album {
                id: "album".to_string(),
                attachment_ids: vec!["first".to_string(), "second".to_string()],
                display_mode: AlbumDisplayMode::StackedCards,
            }],
        };

        assert!(content_matches_attachment_filter(
            &content,
            AttachmentTypeFilter::Image
        ));
        assert!(!content_matches_attachment_filter(
            &content,
            AttachmentTypeFilter::Other
        ));
    }
}
