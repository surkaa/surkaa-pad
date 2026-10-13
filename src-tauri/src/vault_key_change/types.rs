use crate::vault_bootstrap::VaultBootstrap;
use serde::{Deserialize, Serialize};
use specta::Type;

/// 仅包含已加密的本机配置；不会把 OSS/API 密钥明文写入改密记录。
#[derive(Clone, Debug, Default, Deserialize, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalEncryptedConfig {
    pub vault_verifier: Option<Vec<u8>>,
    pub oss: Option<Vec<u8>>,
    pub ai: Option<Vec<u8>>,
}

#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyChangePlan {
    pub plan_id: String,
    pub remote: bool,
    pub total_files: u32,
    #[specta(type = f64)]
    pub total_bytes: u64,
    pub encrypted_files: u32,
    #[specta(type = f64)]
    pub encrypted_bytes: u64,
    #[specta(type = f64)]
    pub temporary_bytes: u64,
    pub derivation_ms: f64,
    pub crypto_estimate_ms: f64,
    #[specta(type = f64)]
    pub transfer_bytes: u64,
    pub new_bootstrap: VaultBootstrap,
}

#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyChangeProgress {
    pub phase: String,
    pub completed_files: u32,
    pub total_files: u32,
    #[specta(type = f64)]
    pub completed_bytes: u64,
    #[specta(type = f64)]
    pub total_bytes: u64,
    pub current_file: Option<String>,
    // 当前文件当前步骤的离散进度；小于 10 MiB 不发送，且不写入恢复记录。
    pub file_progress: Option<KeyChangeFileProgress>,
}

#[derive(Clone, Debug, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum KeyChangeFileStep {
    WritingStage,
    CheckingStageContent,
    CheckingTarget,
    CheckingStage,
    BackingUp,
    CheckingBackup,
    WritingTarget,
    CheckingCommitted,
}

#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyChangeFileProgress {
    pub step: KeyChangeFileStep,
    #[specta(type = f64)]
    pub completed_bytes: u64,
    #[specta(type = f64)]
    pub total_bytes: u64,
    pub completed_points: u32,
    pub total_points: u32,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub(super) struct Entry {
    pub key: String,
    pub size: u64,
    pub etag: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) enum Encoding {
    Gcm,
    Ctr { nonce: Vec<u8> },
    Manifest,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct WorkItem {
    pub source: Entry,
    pub encoding: Encoding,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct VerifiedStage {
    pub size: u64,
    pub old_cipher_hash: String,
    pub new_cipher_hash: String,
    // V1 记录没有这些证据；恢复时不能凭新增字段省略全量检查。
    #[serde(default)]
    pub cloud: Option<CloudStageEvidence>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct CloudStageEvidence {
    pub etag: String,
    pub old_crc64: u64,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Checkpoint {
    pub header_hash: String,
    pub items: Vec<WorkItem>,
    pub next: usize,
    pub staged: Option<VerifiedStage>,
    pub new_etags: std::collections::BTreeMap<String, String>,
    pub local_config: LocalEncryptedConfig,
    /// 已切换配置仍保留记录，直到前端确认本机加密配置也持久保存。
    pub data_committed: bool,
    /// 其他设备改密完成后，本机只更新凭据并丢弃旧缓存，不再改写云端对象。
    #[serde(default)]
    pub refresh_cloud_cache: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Journal {
    pub version: u32,
    pub transaction_id: String,
    pub remote: bool,
    pub old_bootstrap: VaultBootstrap,
    pub new_bootstrap: VaultBootstrap,
    // 只允许新密码恢复旧密钥，绝不能让旧密码经云端记录取得新密钥。
    pub old_key_under_new: Vec<u8>,
    pub encrypted_checkpoint: Vec<u8>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyChangeStatus {
    pub pending: bool,
    pub remote: bool,
}
