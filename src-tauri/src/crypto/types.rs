use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Deserialize, Serialize, Clone, Debug, Type, PartialEq)]
pub enum EncryptionAlgorithm {
    #[serde(rename = "AES256-GCM_v1")]
    Gcm,
    #[serde(rename = "AES-256-CTR")]
    Ctr,
}
