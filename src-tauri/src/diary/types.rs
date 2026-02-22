use crate::attachment::AttachmentMeta;
use crate::crypto::types::EncryptionAlgorithm;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

// Manifest 解密后的 Rust 结构体，代表一篇日记的核心信息
#[derive(Deserialize, Serialize, Clone, Debug, Type)]
pub struct DiaryManifest {
    pub id: String,
    pub algorithm: EncryptionAlgorithm, // 加密算法名称
    pub content: String,                // 日记正文
    #[specta(type = f64)]
    pub created: i64,
    #[specta(type = f64)]
    pub updated: i64,
    pub attachments: Vec<AttachmentMeta>, // 附件列表
}

impl DiaryManifest {
    pub fn content(&self) -> &str {
        &self.content
    }
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
    /// 附件列表
    pub attachments: Vec<AttachmentMeta>,
}

impl DiarySummary {
    pub fn from_manifest(manifest: DiaryManifest) -> Self {
        let mut first_line = manifest.content.lines().next().unwrap_or("").to_string();

        // 循环查找并移除 "<<...>>" 结构
        while let Some(start) = first_line.find("<<") {
            if let Some(end_offset) = first_line[start..].find(">>") {
                // start 是 "<<" 的起始位置，end_offset 是 ">>" 相对 start 的偏移量
                // +2 是为了包含 ">>" 本身的长度
                first_line.replace_range(start..start + end_offset + 2, "");
            } else {
                break; // 如果没有成对的 ">>"，停止处理以防死循环
            }
        }

        let title = first_line.trim().to_string();

        Self {
            id: manifest.id,
            created: manifest.created,
            updated: manifest.updated,
            title,
            attachments: manifest.attachments,
        }
    }
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
    Finished,
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diary_summary_title_from_manifest() {
        let manifest = DiaryManifest {
            id: "1".to_string(),
            algorithm: EncryptionAlgorithm::Gcm,
            content: "My first diary entry<<IMG:filename>>1<<IMG:filename>>\n This is the content.".to_string(),
            created: 0,
            updated: 0,
            attachments: vec![],
        };

        let summary = DiarySummary::from_manifest(manifest.clone());
        assert_eq!(summary.title, "My first diary entry1");
    }
}
