use crate::attachments::AttachmentMeta;
use crate::caches::DiaryMemoryCache;
use crate::cryptos::Crypto;
use crate::diaries::diary_list::page_diary_ids;
use crate::diaries::diary_store::DiaryStore;
use crate::diaries::diary_types::{AttachmentTypeFilter, DiarySummary, SearchDiariesEvent};
use crate::diaries::{get_diary, DiaryError};
use crate::object::NextToken;
use crate::utils::message_sender::MessageSender;
use std::sync::Arc;

pub async fn search_diaries(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    event: Arc<dyn MessageSender<SearchDiariesEvent>>,
    keyword: String,
    or: bool,
    attachment_types: Vec<AttachmentTypeFilter>,
    attachment_or: bool,
) {
    let ec = event.clone();
    let keywords = keyword.split_whitespace().collect::<Vec<_>>();
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

                    let content = diary.content.searchable_text();
                    // 如果 or 是 true，则满足任一关键词即可；如果 or 是 false，则必须满足所有关键词
                    let keyword_matches = if kc.is_empty() {
                        true
                    } else if or {
                        kc.iter().any(|keyword| content.contains(keyword))
                    } else {
                        kc.iter().all(|keyword| content.contains(keyword))
                    };
                    let attachment_matches = attachment_types.is_empty()
                        || if attachment_or {
                            attachment_types.iter().any(|filter| {
                                diary.attachments.iter().any(|attachment| {
                                    attachment_matches_filter(attachment, *filter)
                                })
                            })
                        } else {
                            attachment_types.iter().all(|filter| {
                                diary.attachments.iter().any(|attachment| {
                                    attachment_matches_filter(attachment, *filter)
                                })
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

fn attachment_matches_filter(attachment: &AttachmentMeta, filter: AttachmentTypeFilter) -> bool {
    let mime_group = attachment
        .mimetype
        .split_once('/')
        .map_or(attachment.mimetype.as_str(), |(group, _)| group)
        .to_ascii_lowercase();
    match filter {
        AttachmentTypeFilter::Image => mime_group == "image",
        AttachmentTypeFilter::Audio => mime_group == "audio",
        AttachmentTypeFilter::Video => mime_group == "video",
        AttachmentTypeFilter::Other => !matches!(mime_group.as_str(), "image" | "audio" | "video"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;

    fn attachment(mimetype: &str) -> AttachmentMeta {
        AttachmentMeta {
            id: "att-1".to_string(),
            filename: "1".to_string(),
            mimetype: mimetype.to_string(),
            size: 1,
            encrypted: false,
            nonce: Vec::new(),
            algorithm: Gcm,
            etag: None,
        }
    }

    #[test]
    fn attachment_filter_classifies_mime_groups_case_insensitively() {
        assert!(attachment_matches_filter(
            &attachment("IMAGE/JPEG"),
            AttachmentTypeFilter::Image
        ));
        assert!(attachment_matches_filter(
            &attachment("audio/mpeg"),
            AttachmentTypeFilter::Audio
        ));
        assert!(attachment_matches_filter(
            &attachment("video/mp4"),
            AttachmentTypeFilter::Video
        ));
    }

    #[test]
    fn attachment_filter_treats_files_and_unknown_mime_types_as_other() {
        for mimetype in ["application/pdf", "text/plain", "", "custom"] {
            assert!(attachment_matches_filter(
                &attachment(mimetype),
                AttachmentTypeFilter::Other
            ));
        }
    }
}
