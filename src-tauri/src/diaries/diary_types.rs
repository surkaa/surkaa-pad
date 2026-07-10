use crate::attachments::AttachmentMeta;
use crate::cryptos::crypto_types::EncryptionAlgorithm;
use serde::{Deserialize, Serialize};
use specta::Type;

const fn default_version() -> u32 { 1 }

// Manifest 解密后的 Rust 结构体，代表一篇日记的核心信息
#[derive(Deserialize, Serialize, Clone, Debug, Type)]
pub struct DiaryManifest {
    pub id: String,
    pub algorithm: EncryptionAlgorithm, // 加密算法名称
    pub content: String,                // 日记正文
    // TODO(V3): content 字段改为 JSON 格式，支持结构化内容
    //   - 引入图集概念：多张图片组成一个图集（Album），一个日记可有多个图集
    //   - 图集显示模式：左右列表 / 堆叠卡片式（微信聊天发多图效果，多张图片堆叠成扑克牌状，
    //     点击最上面的图片将其移到底部，逐一切换查看）
    //   - 涉及 manifest 版本号升级（V2→V3）及 diary_migration 迁移步骤
    #[specta(type = f64)]
    pub created: i64,
    #[specta(type = f64)]
    pub updated: i64,
    pub attachments: Vec<AttachmentMeta>, // 附件列表
    #[serde(default = "default_version")]
    pub version: u32,
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
