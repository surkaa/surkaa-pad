use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum DownloadAttachmentEvent {
    Started { total_size: u64 },
    DownloadProgress { downloaded: u64 },
    Decrypting,
    Decrypted { decrypted_size: u64 },
    Completed { file_path: String },
    Error { message: String },
}

// 单个附件的元数据
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mimetype: String,
    pub size: u64,
    pub nonce: Vec<u8>, // 用于加密该文件的独立 IV
}
