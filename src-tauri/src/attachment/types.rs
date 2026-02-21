use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum AddAttachmentEvent {
    Started,
    /// 0-100 的上传进度百分比
    Progress(u8),
    Completed(AttachmentMeta),
    Error(String),
}

#[derive(Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum DownloadAttachmentEvent {
    Started,
    /// 0-100 的下载进度百分比
    Progress(u8),
    /// 下载完成，保存在应用目录AppData下的路径
    Completed(String),
    Error(String),
}

// 单个附件的元数据
#[derive(Deserialize, Serialize, Clone, Debug, Type)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mimetype: String,
    #[specta(type = f64)]
    pub size: u64,
    pub nonce: Vec<u8>, // 用于加密该文件的独立 IV
}
