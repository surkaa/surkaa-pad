use crate::attachments::AttachmentMeta;
use crate::cryptos::crypto_types::EncryptionAlgorithm;
use crate::diaries::diary_content::{DiaryAttachmentCounts, DiaryContent};
use serde::{Deserialize, Serialize};
use specta::Type;

const fn default_version() -> u32 {
    1
}

// Manifest 解密后的 Rust 结构体，代表一篇日记的核心信息
#[derive(Deserialize, Serialize, Clone, Debug, Type)]
pub struct DiaryManifest {
    pub id: String,
    pub algorithm: EncryptionAlgorithm, // 加密算法名称
    pub content: DiaryContent,
    #[specta(type = f64)]
    pub created: i64,
    #[specta(type = f64)]
    pub updated: i64,
    pub attachments: Vec<AttachmentMeta>, // 附件列表
    #[serde(default = "default_version")]
    pub version: u32,
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
    /// 正文节点中各类附件的数量，不包含未插入正文的附件
    pub attachment_counts: DiaryAttachmentCounts,
}

impl DiarySummary {
    pub fn from_manifest(manifest: DiaryManifest) -> Self {
        let title = manifest.content.title();
        let attachment_counts = manifest.content.attachment_counts();

        Self {
            id: manifest.id,
            created: manifest.created,
            updated: manifest.updated,
            title,
            attachments: manifest.attachments,
            attachment_counts,
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentTypeFilter {
    Image,
    Audio,
    Video,
    Other,
}

#[cfg(test)]
mod tests {
    use super::{DiaryManifest, DiarySummary};
    use crate::attachments::AttachmentMeta;
    use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
    use crate::diaries::diary_content::{DiaryAttachmentCounts, DiaryContent, DiaryContentNode};

    #[test]
    fn summary_counts_content_nodes_instead_of_attachment_metadata() {
        let summary = DiarySummary::from_manifest(DiaryManifest {
            id: "diary-1".to_string(),
            algorithm: Gcm,
            content: DiaryContent {
                nodes: vec![DiaryContentNode::Audio {
                    attachment_id: "audio-1".to_string(),
                }],
            },
            created: 1,
            updated: 2,
            attachments: vec![AttachmentMeta {
                id: "unused-image".to_string(),
                filename: "unused.jpg".to_string(),
                mimetype: "image/jpeg".to_string(),
                size: 1,
                encrypted: true,
                nonce: Vec::new(),
                algorithm: Gcm,
                etag: None,
            }],
            version: 4,
        });

        assert_eq!(
            summary.attachment_counts,
            DiaryAttachmentCounts {
                image: 0,
                audio: 1,
                video: 0,
                file: 0,
            }
        );
    }
}
