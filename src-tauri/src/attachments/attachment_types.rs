use crate::cryptos::crypto_types::EncryptionAlgorithm;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum AttachmentProcessEvent {
    Started,
    /// 0-100 的上传进度百分比
    Progress(u8),
    /// 返回附件的元数据和访问URL
    Completed(AttachmentMeta, String),
    /// 不返回数据但仍成功的场景
    CompletedWithoutData,
    Error(String),
}

// 单个附件的元数据
#[derive(Deserialize, Serialize, Clone, Debug, Type)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mimetype: String,
    #[specta(type = f64)]
    pub size: u64,
    pub encrypted: bool,
    pub nonce: Vec<u8>, // 用于加密该文件的独立 IV
    pub algorithm: EncryptionAlgorithm,
    #[serde(default)]
    pub etag: Option<String>,
}
