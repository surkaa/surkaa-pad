use crate::cryptos::crypto_types::Aes256Ctr;
use crate::diaries::{AttachmentUploadSession, DiaryStore};
use tokio::sync::OwnedRwLockReadGuard;

/// 分片上传的中间状态，存储在 AppState 的 DashMap 中
pub struct ChunkedUploadState {
    pub diary_id: String,
    pub attachment_id: String,
    pub filename: String,
    pub mimetype: String,
    pub encrypted: bool,
    pub nonce: Vec<u8>,
    /// CTR cipher 实例（仅加密时有值）；整个状态由异步 Mutex 串行访问。
    pub cipher: Option<Aes256Ctr>,
    pub session: Option<Box<dyn AttachmentUploadSession>>,
    pub store: Box<dyn DiaryStore>,
    /// 保持上传期间存储模式稳定，完成或取消时随状态一起释放。
    pub _storage_mode_guard: OwnedRwLockReadGuard<()>,
    pub total_size: u64,
    pub uploaded_bytes: u64,
    pub next_part_number: u32,
}
