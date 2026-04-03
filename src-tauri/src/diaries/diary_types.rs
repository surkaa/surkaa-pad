use crate::attachments::AttachmentMeta;
use crate::cryptos::crypto_types::EncryptionAlgorithm;
use serde::{Deserialize, Serialize};
use specta::Type;

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
        let title = manifest.content.lines().next().unwrap_or("").trim().to_string();

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
            content: "My first diary entry[[IMG:filename]]1[[IMG:filename]]\n This is the content."
                .to_string(),
            created: 0,
            updated: 0,
            attachments: vec![],
        };

        let summary = DiarySummary::from_manifest(manifest.clone());
        assert_eq!(summary.title, "My first diary entry1");
    }
}
