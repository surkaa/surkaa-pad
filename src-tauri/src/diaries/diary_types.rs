use crate::attachments::AttachmentMeta;
use crate::cryptos::crypto_types::EncryptionAlgorithm;
use crate::diaries::diary_content::{DiaryAttachmentCounts, DiaryContent};
use crate::diaries::DiaryError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::collections::{HashMap, HashSet};

/// 当前代码唯一支持的日记 Manifest 版本。
pub const CURRENT_VERSION: u32 = 6;

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
    pub version: u32,
}

impl DiaryManifest {
    pub(crate) fn matches_keywords(&self, keywords: &[String], match_any: bool) -> bool {
        if keywords.is_empty() {
            return true;
        }

        let searchable_text = self.content.searchable_text().to_lowercase();
        let searchable_filenames = self
            .attachments
            .iter()
            .map(|attachment| attachment.filename.to_lowercase())
            .collect::<Vec<_>>();
        let matches_keyword = |keyword: &str| {
            let keyword = keyword.to_lowercase();
            searchable_text.contains(&keyword)
                || searchable_filenames
                    .iter()
                    .any(|filename| filename.contains(&keyword))
        };
        if match_any {
            keywords
                .iter()
                .any(|keyword| matches_keyword(keyword.as_str()))
        } else {
            keywords
                .iter()
                .all(|keyword| matches_keyword(keyword.as_str()))
        }
    }
}

/// 解析 Manifest 的身份和版本元数据，不要求旧版或高版本符合当前结构。
pub(crate) fn inspect_manifest_json(
    requested_id: &str,
    manifest_bytes: &[u8],
) -> Result<(Value, u32), DiaryError> {
    let json: Value = serde_json::from_slice(manifest_bytes)?;
    let manifest_id = json
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| DiaryError::InvalidManifest("Manifest diary id is missing".to_string()))?;
    if manifest_id != requested_id {
        return Err(DiaryError::InvalidManifest(format!(
            "Manifest diary id {manifest_id} does not match requested id {requested_id}"
        )));
    }

    // 历史 V1 没有 version 字段；这里只识别它，正常读取不再兼容它。
    let version = match json.get("version") {
        None => 1,
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|version| u32::try_from(version).ok())
            .filter(|version| *version > 0)
            .ok_or_else(|| {
                DiaryError::InvalidManifest("Manifest version must be a positive u32".to_string())
            })?,
        Some(_) => {
            return Err(DiaryError::InvalidManifest(
                "Manifest version must be an integer".to_string(),
            ));
        }
    };

    Ok((json, version))
}

pub(crate) fn deserialize_current_manifest(
    requested_id: &str,
    manifest_bytes: &[u8],
) -> Result<DiaryManifest, DiaryError> {
    let (json, version) = inspect_manifest_json(requested_id, manifest_bytes)?;
    if version != CURRENT_VERSION {
        return Err(DiaryError::UnsupportedVersion {
            found: version,
            supported: CURRENT_VERSION,
        });
    }
    Ok(serde_json::from_value(json)?)
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
    /// Manifest 中所有附件声明大小的总和，包含未插入正文的附件
    #[specta(type = f64)]
    pub attachment_total_size: u64,
    /// 正文节点中各类附件的数量，不包含未插入正文的附件
    pub attachment_counts: DiaryAttachmentCounts,
    /// 正文节点中各类加密附件的数量
    pub encrypted_attachment_counts: DiaryAttachmentCounts,
}

impl DiarySummary {
    pub fn from_manifest(manifest: &DiaryManifest) -> Self {
        let title = manifest.content.title();
        let attachment_counts = manifest.content.attachment_counts();
        let attachment_total_size = manifest.attachments.iter().fold(0u64, |total, attachment| {
            total.saturating_add(attachment.size)
        });
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
            attachment_total_size,
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
    #[specta(type = f64)]
    pub manifest_size: u64,
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
    use super::{deserialize_current_manifest, DiaryManifest, DiarySummary, CURRENT_VERSION};
    use crate::attachments::AttachmentMeta;
    use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
    use crate::diaries::diary_content::{DiaryAttachmentCounts, DiaryContent, DiaryContentNode};

    fn searchable_manifest() -> DiaryManifest {
        DiaryManifest {
            id: "diary-1".to_string(),
            algorithm: Gcm,
            content: DiaryContent::from("上海出差记录"),
            created: 1,
            updated: 2,
            attachments: ["酒店发票.pdf", "行程.png"]
                .into_iter()
                .map(|filename| AttachmentMeta {
                    id: filename.to_string(),
                    filename: filename.to_string(),
                    mimetype: "application/octet-stream".to_string(),
                    size: 1,
                    encrypted: false,
                    nonce: Vec::new(),
                    algorithm: Gcm,
                    etag: None,
                })
                .collect(),
            version: CURRENT_VERSION,
        }
    }

    #[test]
    fn keywords_match_attachment_filenames() {
        let manifest = searchable_manifest();

        assert!(manifest.matches_keywords(&["酒店发票".into()], false));
        assert!(!manifest.matches_keywords(&["会议记录".into()], true));
    }

    #[test]
    fn keyword_logic_combines_body_and_filenames() {
        let manifest = searchable_manifest();

        assert!(manifest.matches_keywords(&["上海".into(), "发票".into()], false));
        assert!(!manifest.matches_keywords(&["上海".into(), "机票".into()], false));
        assert!(manifest.matches_keywords(&["机票".into(), "行程".into()], true));
        assert!(manifest.matches_keywords(&[], false));
    }

    #[test]
    fn keyword_matching_ignores_case_in_body_and_filenames() {
        let mut manifest = searchable_manifest();
        manifest.content = DiaryContent::from("Rust Search Notes");
        manifest.attachments[0].filename = "Quarterly-REPORT.PDF".to_string();

        assert!(manifest.matches_keywords(&["rust search".into()], false));
        assert!(manifest.matches_keywords(&["report.pdf".into()], false));
    }

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
                    size: 1_024,
                    encrypted: true,
                    nonce: Vec::new(),
                    algorithm: Gcm,
                    etag: None,
                },
                AttachmentMeta {
                    id: "file-1".to_string(),
                    filename: "plain.txt".to_string(),
                    mimetype: "text/plain".to_string(),
                    size: 2_048,
                    encrypted: false,
                    nonce: Vec::new(),
                    algorithm: Gcm,
                    etag: None,
                },
                AttachmentMeta {
                    id: "unused-image".to_string(),
                    filename: "unused.jpg".to_string(),
                    mimetype: "image/jpeg".to_string(),
                    size: 4_096,
                    encrypted: true,
                    nonce: Vec::new(),
                    algorithm: Gcm,
                    etag: None,
                },
            ],
            version: CURRENT_VERSION,
        };
        let summary = DiarySummary::from_manifest(&manifest);

        assert_eq!(summary.attachment_count, 3);
        assert_eq!(summary.attachment_total_size, 7_168);
        let serialized = serde_json::to_value(&summary).expect("serialize summary");
        assert_eq!(serialized["attachmentCount"], 3);
        assert_eq!(serialized["attachmentTotalSize"], 7_168);
        assert!(serialized.get("attachments").is_none());

        let full_manifest = serde_json::to_value(&manifest).expect("serialize manifest");
        assert_eq!(full_manifest["id"], "diary-1");
        assert_eq!(full_manifest["version"], CURRENT_VERSION);
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

    #[test]
    fn current_manifest_parser_requires_matching_id_and_exact_version() {
        let manifest = DiaryManifest {
            id: "current".to_string(),
            algorithm: Gcm,
            content: DiaryContent::default(),
            created: 1,
            updated: 1,
            attachments: Vec::new(),
            version: CURRENT_VERSION,
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        assert_eq!(
            deserialize_current_manifest("current", &bytes)
                .unwrap()
                .version,
            CURRENT_VERSION
        );
        assert!(matches!(
            deserialize_current_manifest("other", &bytes),
            Err(crate::diaries::DiaryError::InvalidManifest(message))
                if message.contains("does not match")
        ));

        for (source, expected_version) in [
            (serde_json::json!({"id": "legacy"}), 1),
            (serde_json::json!({"id": "legacy", "version": 3}), 3),
            (
                serde_json::json!({"id": "legacy", "version": CURRENT_VERSION + 1}),
                CURRENT_VERSION + 1,
            ),
        ] {
            let bytes = serde_json::to_vec(&source).unwrap();
            assert!(matches!(
                deserialize_current_manifest("legacy", &bytes),
                Err(crate::diaries::DiaryError::UnsupportedVersion { found, supported })
                    if found == expected_version && supported == CURRENT_VERSION
            ));
        }
    }
}
