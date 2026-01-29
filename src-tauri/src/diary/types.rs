use crate::attachment::AttachmentMeta;
use serde::{Deserialize, Serialize};

// Manifest 解密后的 Rust 结构体，代表一篇日记的核心信息
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DiaryManifest {
    pub id: String,
    pub algorithm: String, // 加密算法名称
    pub content: String,   // 日记正文
    pub created: i64,
    pub updated: i64,
    pub attachments: Vec<AttachmentMeta>, // 附件列表
}
