use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum DownloadAttachmentEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        #[specta(type = f64)]
        total_size: u64,
    },
    DownloadProgress {
        #[specta(type = f64)]
        downloaded: u64,
    },
    Decrypting,
    #[serde(rename_all = "camelCase")]
    Decrypted {
        #[specta(type = f64)]
        decrypted_size: u64,
    },
    #[serde(rename_all = "camelCase")]
    Completed {
        file_path: String,
    },
    Error {
        message: String,
    },
}

// 单个附件的元数据
#[derive(Deserialize, Serialize, Clone, Debug, Type)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mimetype: String,
    #[specta(type = f64)]
    pub size: u64,
    pub nonce: Vec<u8>, // 用于加密该文件的独立 IV
}
