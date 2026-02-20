use crate::crypto::Crypto;
use crate::diary::types::SearchDiariesEvent;
use crate::diary::DiaryMemoryCache;
use crate::object::OssClient;
use crate::utils::message_sender::MessageSender;
use std::sync::Arc;

pub(super) async fn search_diaries(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    event: Arc<dyn MessageSender<SearchDiariesEvent>>,
    keyword: String,
) {
    todo!()
}
