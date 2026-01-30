use crate::attachment::AttachmentMeta;
use serde::{Deserialize, Serialize};
use specta::Type;

// Manifest 解密后的 Rust 结构体，代表一篇日记的核心信息
#[derive(Deserialize, Serialize, Clone, Debug, Type)]
pub struct DiaryManifest {
    pub id: String,
    pub algorithm: String, // 加密算法名称
    pub content: String,   // 日记正文
    #[specta(type = f64)]
    pub created: i64,
    #[specta(type = f64)]
    pub updated: i64,
    pub attachments: Vec<AttachmentMeta>, // 附件列表
}
