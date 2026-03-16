use super::crypto_types::{
    Aes256Ctr, DerivedKey, CTR_NONCE_LEN, KEY_LEN, MEMORY_COST_KIB, NONCE_LEN,
};
use crate::stream::ByteStream;
use aes::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::aes::cipher::crypto_common::rand_core::RngCore;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, ParamsBuilder, PasswordHasher, Version};
use bytes::Bytes;
use futures_util::StreamExt;
use std::sync::{Arc, OnceLock};
use zeroize::Zeroize;

impl std::ops::Deref for DerivedKey {
    type Target = [u8; KEY_LEN];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct InnerCrypto {
    dek: OnceLock<DerivedKey>,
}

#[derive(Clone)]
pub struct Crypto {
    inner: Arc<InnerCrypto>,
}

impl Crypto {
    pub fn new() -> Self {
        Crypto {
            inner: Arc::new(InnerCrypto {
                dek: OnceLock::new(),
            }),
        }
    }

    #[cfg(test)]
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        let password = std::env::var("TEST_PASSWORD").expect("TEST_PASSWORD 未设置");
        let salt = std::env::var("TEST_SALT").expect("TEST_PASSWORD 未设置");
        let crypto = Crypto::new();
        let _ = crypto.derive_dek(password, salt).expect("派生密钥失败");
        crypto
    }

    /// 利用提供的派生密钥解密给定数据
    pub fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>, String> {
        // 从前NONCE_LEN字节中提取nonce
        if encrypted.len() < NONCE_LEN {
            return Err("密文长度不足以包含 nonce".to_string());
        }
        let (nonce_bytes, ciphertext) = encrypted.split_at(NONCE_LEN);
        let dek = self.inner.dek.get().ok_or("未派生密钥".to_string())?;
        let cipher =
            Aes256Gcm::new_from_slice(dek.as_ref()).map_err(|e| format!("无效的密钥: {:?}", e))?;

        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("解密失败: {:?}", e))?;

        Ok(plaintext)
    }

    /// 使用CTR流式加密包装流
    pub fn encrypt_streaming(&self, stream: ByteStream) -> Result<(ByteStream, Vec<u8>), String> {
        let dek = self.inner.dek.get().ok_or("未派生密钥".to_string())?;

        // 生成随机NONCE
        let mut nonce = [0u8; CTR_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);

        let cipher = Aes256Ctr::new(dek.as_ref().into(), (&nonce).into());

        let mapped_stream = ctr_stream_cipher(stream, cipher);

        Ok((mapped_stream, nonce.to_vec()))
    }

    /// 使用CTR流式解密
    pub fn decrypt_streaming(
        &self,
        stream: ByteStream,
        nonce: &[u8],
        start_offset: u64,
    ) -> Result<ByteStream, String> {
        let dek = self.inner.dek.get().ok_or("未派生密钥".to_string())?;

        if nonce.len() != CTR_NONCE_LEN {
            return Err("NONCE 长度错误".to_string());
        }

        let mut cipher = Aes256Ctr::new(dek.as_ref().into(), nonce.into());

        // 计算初始计数器值：start_offset / 16（块大小）
        cipher.seek(start_offset);

        let mapped_stream = ctr_stream_cipher(stream, cipher);

        Ok(mapped_stream)
    }

    /// 使用提供的派生密钥对给定数据进行加密。
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let dek = self.inner.dek.get().ok_or("未派生密钥".to_string())?;
        let cipher =
            Aes256Gcm::new_from_slice(dek.as_ref()).map_err(|e| format!("无效的密钥: {:?}", e))?;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| format!("加密失败: {:?}", e))?;

        Ok([nonce_bytes.to_vec(), ciphertext].concat())
    }

    /// 从给定的密码和盐中推导出数据加密密钥（DEK）
    pub fn derive_dek(&self, mut password: String, salt: String) -> Result<String, String> {
        let params = ParamsBuilder::new()
            .t_cost(2)
            .m_cost(MEMORY_COST_KIB)
            .p_cost(4)
            .output_len(KEY_LEN)
            .build()
            .map_err(|e| format!("参数错误:{}", e))?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let salt = SaltString::from_b64(salt.as_str()).map_err(|e| format!("非法盐:{}", e))?;

        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| format!("派生失败:{}", e))?;

        password.zeroize();

        let dek = hash.hash.ok_or("无法提取哈希值".to_string())?;
        if dek.as_bytes().len() == KEY_LEN {
            let mut dek_array = [0u8; KEY_LEN];
            dek_array.copy_from_slice(dek.as_bytes());
            let derived_key = DerivedKey(dek_array);
            let _ = self.inner.dek.set(derived_key);
            let dek_string = hex::encode(dek_array);
            dek_array.zeroize();
            Ok(dek_string)
        } else {
            Err("派生的密钥长度不正确".to_string())
        }
    }

    /// 使用密钥字符串初始化
    pub fn init_by_dek_string(&self, dek: String) -> Result<(), String> {
        let dek_bytes: Vec<u8> =
            hex::decode(&dek).map_err(|e| format!("Failed to decode DEK: {}", e))?;
        if dek_bytes.len() != KEY_LEN {
            return Err("Invalid DEK length".to_string());
        }
        let mut dek_array = [0u8; KEY_LEN];
        dek_array.copy_from_slice(&dek_bytes);
        let derived_key = DerivedKey(dek_array);
        let _ = self.inner.dek.set(derived_key);
        dek_array.zeroize();
        Ok(())
    }
}

fn ctr_stream_cipher(stream: ByteStream, mut cipher: Aes256Ctr) -> ByteStream {
    Box::pin(stream.map(move |result| match result {
        Ok(bytes) => {
            let mut buffer = bytes.to_vec();
            cipher.apply_keystream(&mut buffer);
            Ok(Bytes::from(buffer))
        }
        Err(e) => Err(e),
    }))
}
