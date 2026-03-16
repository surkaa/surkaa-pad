use serde::{Deserialize, Serialize};
use specta::Type;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// 定义 AES-GCM 的 nonce 长度
pub const NONCE_LEN: usize = 12;

/// 定义派生密钥的长度（字节），AES-256 需要 32 字节
pub const KEY_LEN: usize = 32;

/// 定义内存成本（KiB）
#[cfg(debug_assertions)]
pub const MEMORY_COST_KIB: u32 = 1024;
#[cfg(not(debug_assertions))]
pub const MEMORY_COST_KIB: u32 = 256 * 1024;

// 定义 AES-256-CTR 类型 (128BE代表128位大端序计数器)
pub type Aes256Ctr = ctr::Ctr128BE<aes::Aes256>;

// 定义 CTR 模式的 NONCE(IV) 长度
pub const CTR_NONCE_LEN: usize = 16;

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct DerivedKey(pub [u8; KEY_LEN]);

#[derive(Deserialize, Serialize, Clone, Debug, Type, PartialEq)]
pub enum EncryptionAlgorithm {
    #[serde(rename = "AES256-GCM_v1")]
    Gcm,
    #[serde(rename = "AES-256-CTR")]
    Ctr,
}
