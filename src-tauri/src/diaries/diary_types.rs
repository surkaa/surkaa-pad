use crate::attachments::AttachmentMeta;
use crate::cryptos::crypto_types::EncryptionAlgorithm;
use crate::diaries::diary_content::{DiaryAttachmentCounts, DiaryContent};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};

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
    /// Manifest 中的附件总数，包含未插入正文的附件
    #[specta(type = f64)]
    pub attachment_count: usize,
    /// 正文节点中各类附件的数量，不包含未插入正文的附件
    pub attachment_counts: DiaryAttachmentCounts,
    /// 正文节点中各类加密附件的数量
    pub encrypted_attachment_counts: DiaryAttachmentCounts,
}

impl DiarySummary {
    pub fn from_manifest(manifest: &DiaryManifest) -> Self {
        let title = manifest.content.title();
        let attachment_counts = manifest.content.attachment_counts();
        let encrypted_attachment_ids = manifest
            .attachments
            .iter()
            .filter(|attachment| attachment.encrypted)
            .map(|attachment| attachment.id.as_str())
            .collect::<HashSet<_>>();
        let encrypted_attachment_counts = manifest
            .content
            .attachment_counts_for_ids(&encrypted_attachment_ids);

        Self {
            id: manifest.id.clone(),
            created: manifest.created,
            updated: manifest.updated,
            title,
            attachment_count: manifest.attachments.len(),
            attachment_counts,
            encrypted_attachment_counts,
        }
    }
}

/// 仅在进入日记编辑页后加载的完整详情。
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiaryDetail {
    pub summary: DiarySummary,
    pub content: DiaryContent,
    pub attachments: Vec<AttachmentMeta>,
    pub attachment_urls: HashMap<String, String>,
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
        let manifest = DiaryManifest {
            id: "diary-1".to_string(),
            algorithm: Gcm,
            content: DiaryContent {
                nodes: vec![
                    DiaryContentNode::Audio {
                        attachment_id: "audio-1".to_string(),
                    },
                    DiaryContentNode::File {
                        attachment_id: "file-1".to_string(),
                    },
                ],
            },
            created: 1,
            updated: 2,
            attachments: vec![
                AttachmentMeta {
                    id: "audio-1".to_string(),
                    filename: "audio.m4a".to_string(),
                    mimetype: "video/mp4".to_string(),
                    size: 1,
                    encrypted: true,
                    nonce: Vec::new(),
                    algorithm: Gcm,
                    etag: None,
                },
                AttachmentMeta {
                    id: "file-1".to_string(),
                    filename: "plain.txt".to_string(),
                    mimetype: "text/plain".to_string(),
                    size: 1,
                    encrypted: false,
                    nonce: Vec::new(),
                    algorithm: Gcm,
                    etag: None,
                },
                AttachmentMeta {
                    id: "unused-image".to_string(),
                    filename: "unused.jpg".to_string(),
                    mimetype: "image/jpeg".to_string(),
                    size: 1,
                    encrypted: true,
                    nonce: Vec::new(),
                    algorithm: Gcm,
                    etag: None,
                },
            ],
            version: 4,
        };
        let summary = DiarySummary::from_manifest(&manifest);

        assert_eq!(summary.attachment_count, 3);
        let serialized = serde_json::to_value(&summary).expect("serialize summary");
        assert_eq!(serialized["attachmentCount"], 3);
        assert!(serialized.get("attachments").is_none());

        let full_manifest = serde_json::to_value(&manifest).expect("serialize manifest");
        assert_eq!(full_manifest["id"], "diary-1");
        assert_eq!(full_manifest["version"], 4);
        assert_eq!(full_manifest["algorithm"], "AES256-GCM_v1");
        assert_eq!(
            full_manifest["content"]["nodes"].as_array().unwrap().len(),
            2
        );
        assert_eq!(full_manifest["attachments"].as_array().unwrap().len(), 3);

        assert_eq!(
            summary.attachment_counts,
            DiaryAttachmentCounts {
                image: 0,
                audio: 1,
                video: 0,
                file: 1,
            }
        );
        assert_eq!(
            summary.encrypted_attachment_counts,
            DiaryAttachmentCounts {
                image: 0,
                audio: 1,
                video: 0,
                file: 0,
            }
        );
    }
}
