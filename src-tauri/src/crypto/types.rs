use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Deserialize, Serialize, Clone, Debug, Type, PartialEq)]
pub enum EncryptionAlgorithm {
    #[serde(rename = "AES-256-GCM")]
    Gcm,
    #[serde(rename = "AES-256-CTR")]
    Ctr,
}

// 实现 Default trait，用于兼容旧数据
impl Default for EncryptionAlgorithm {
    fn default() -> Self {
        Self::Gcm
    }
}
