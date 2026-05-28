use crate::caches::ChunkedSaveHandle;
use crate::cryptos::crypto_types::Aes256Ctr;

/// 分片上传的中间状态，存储在 AppState 的 DashMap 中
pub struct ChunkedUploadState {
    pub diary_id: String,
    pub allocated_id: u32,
    pub allocated_filename: String,
    pub upload_id: String,
    pub key: String,
    pub filename: String,
    pub mimetype: String,
    pub encrypted: bool,
    pub nonce: Vec<u8>,
    /// CTR cipher 实例（仅加密时有值），用 std::sync::Mutex 包装以满足 DashMap 的 Sync 要求
    pub cipher: Option<std::sync::Mutex<Aes256Ctr>>,
    /// 已上传分片的 (etag, part_number) 列表
    pub parts: Vec<(String, u32)>,
    pub lfc_handle: ChunkedSaveHandle,
    pub total_size: u64,
    pub uploaded_bytes: u64,
    pub next_part_number: u32,
}
