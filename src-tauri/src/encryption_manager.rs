use aead::{Aead, OsRng};
use aes_gcm::aes::cipher::crypto_common::rand_core::RngCore;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, ParamsBuilder, PasswordHasher, Version};

// 定义常量
const NONCE_LEN: usize = 12;
// 定义派生密钥的长度（字节），AES-256 需要 32 字节
const KEY_LEN: usize = 32;

pub struct EncryptionManager {
    dek: Vec<u8>,
    pub algorithm: String,
}

/// 加密算法管理器实现 只用于加密和解密数据
impl EncryptionManager {
    pub fn new() -> Self {
        EncryptionManager {
            dek: Vec::new(), // 初始化为空
            algorithm: "AES256-GCM_v1".to_string(),
        }
    }

    pub fn initial(&mut self, master_password: &str, salt: &str) -> Result<(), String> {
        let memory_cost_kib = 1024 * 256;

        let params = ParamsBuilder::new()
            .t_cost(2)
            .m_cost(memory_cost_kib)
            .p_cost(4)
            .output_len(KEY_LEN)
            .build()
            .map_err(|e| format!("Argon2 参数错误: {}", e))?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let salt = SaltString::from_b64(salt)
            .map_err(|e| format!("Salt 字符串无效或不是 Base64 编码: {}", e))?;

        let hash = argon2
            .hash_password(master_password.as_bytes(), &salt)
            .map_err(|e| format!("密钥派生失败: {}", e))?;

        let dek = hash.hash.ok_or_else(|| "无法提取哈希值".to_string())?;

        if dek.as_bytes().len() != KEY_LEN {
            return Err("派生的密钥长度不正确".to_string());
        }

        // 存储 DEK
        self.dek = dek.as_bytes().to_vec();

        Ok(())
    }

    /// 使用 DEK 加密数据，返回: (密文, nonce)
    pub async fn encrypt(&self, data: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
        let cipher =
            Aes256Gcm::new_from_slice(&self.dek).map_err(|_| "DEK 长度错误".to_string())?;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| format!("加密失败: {:?}", e))?;

        // 返回密文和随机生成的 nonce
        Ok((ciphertext, nonce_bytes.to_vec()))
    }

    /// 使用 DEK 和提供的 nonce 解密数据
    pub async fn decrypt(&self, ciphertext: &[u8], nonce_bytes: &[u8]) -> Result<Vec<u8>, String> {
        let cipher =
            Aes256Gcm::new_from_slice(&self.dek).map_err(|_| "DEK 长度错误".to_string())?;
        if nonce_bytes.len() != NONCE_LEN {
            return Err("IV 长度不正确".to_string());
        }
        let nonce = Nonce::from_slice(nonce_bytes);
        let decrypted_bytes = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("解密失败: {:?}", e))?;
        Ok(decrypted_bytes)
    }

    /// 解密包含 nonce 的密文，假设前面有 NONCE_LEN 字节是 nonce
    pub async fn decrypt_from_full_ciphertext(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        // 假设前面有 NONCE_LEN 字节是 nonce
        if ciphertext.len() < NONCE_LEN {
            return Err("密文长度不足，无法提取 nonce".to_string());
        }
        // 分割 nonce 和实际密文
        let (nonce_bytes, actual_ciphertext) = ciphertext.split_at(NONCE_LEN);
        self.decrypt(actual_ciphertext, nonce_bytes).await
    }
}
