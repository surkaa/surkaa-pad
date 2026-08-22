use crate::cryptos::crypto_types::EncryptionAlgorithm;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum AttachmentProcessEvent {
    Started,
    /// 0-100 的上传进度百分比
    Progress(u8),
    /// 数据已传输完毕，正在提交存储事务并更新日记。
    Finalizing,
    /// 返回附件的元数据和访问URL
    Completed(AttachmentMeta, String),
    /// 不返回数据但仍成功的场景
    CompletedWithoutData,
    Error(String),
}

// 单个附件的元数据
#[derive(Deserialize, Serialize, Clone, Debug, Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMeta {
    /// 稳定附件 ID，同时作为存储对象 key 的末段。
    pub id: String,
    /// 仅用于展示和导出，不再参与对象寻址。
    pub filename: String,
    pub mimetype: String,
    #[specta(type = f64)]
    pub size: u64,
    pub encrypted: bool,
    pub nonce: Vec<u8>, // 用于加密该文件的独立 IV
    pub algorithm: EncryptionAlgorithm,
    #[serde(default)]
    pub etag: Option<String>,
    /// 从附件内容中提取、可重新生成的类型专属信息。
    #[serde(default)]
    pub content_info: Option<AttachmentContentInfo>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum AttachmentContentInfo {
    Audio {
        #[specta(type = f64)]
        duration_ms: Option<u64>,
        waveform: Option<AudioWaveform>,
    },
    Image {
        width: Option<u32>,
        height: Option<u32>,
        frame_count: Option<u32>,
        #[specta(type = f64)]
        duration_ms: Option<u64>,
    },
    Video {
        width: Option<u32>,
        height: Option<u32>,
        #[specta(type = f64)]
        duration_ms: Option<u64>,
    },
    Archive {
        format: Option<String>,
        #[specta(type = f64)]
        entry_count: Option<u64>,
        #[specta(type = f64)]
        uncompressed_size: Option<u64>,
    },
}

/// 用于语音条渲染的紧凑单声道振幅包络。
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioWaveform {
    /// 音波生成算法版本；算法变化时可据此重新生成。
    pub version: u8,
    /// 归一化到 0..=255 的振幅峰值。
    pub peaks: Vec<u8>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChunkedUploadStartResult {
    pub upload_token: String,
    pub attachment_id: String,
    pub filename: String,
    pub nonce: Option<Vec<u8>>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChunkedUploadChunkResult {
    pub part_number: u32,
    pub etag: String,
    pub uploaded_bytes: f64,
    pub total_bytes: f64,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChunkedUploadFinishResult {
    pub attachment: AttachmentMeta,
    pub url: String,
}
