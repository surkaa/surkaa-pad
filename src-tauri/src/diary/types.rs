use crate::attachment::AttachmentMeta;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

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

#[derive(Deserialize, Serialize, Clone, Debug, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiarySummary {
    pub id: String,
    #[specta(type = f64)]
    pub created: i64,
    #[specta(type = f64)]
    pub updated: i64,
    /// 日记标题，取自正文的第一行
    pub title: String,
    /// 附件Map，key：IMG、AUD、VID，value：AttachmentMeta
    pub attachment_map: HashMap<String, AttachmentMeta>,
}

#[derive(Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum SearchDiariesEvent {
    Match(DiarySummary),
    Unmatch(String),
    Error(String),
}