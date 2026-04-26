use super::crypto_types::{
    Aes256Ctr, DerivedKey, CTR_NONCE_LEN, KEY_LEN, MEMORY_COST_KIB, NONCE_LEN,
};
use crate::cryptos::CryptoError;
use crate::stream::ByteStream;
use aes::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::aes::cipher::crypto_common::rand_core::RngCore;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, ParamsBuilder, PasswordHasher, Version};
use bytes::Bytes;
use futures_util::StreamExt;
use std::sync::{Arc, RwLock};
use zeroize::Zeroize;

impl std::ops::Deref for DerivedKey {
    type Target = [u8; KEY_LEN];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub struct Crypto {
    inner: Arc<RwLock<Option<DerivedKey>>>,
}

impl Crypto {
    pub fn new() -> Self {
        Crypto {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    #[cfg(test)]
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        let password = std::env::var("TEST_PASSWORD").expect("TEST_PASSWORD 未设置");
        let salt = std::env::var("TEST_SALT").expect("TEST_PASSWORD 未设置");
        let crypto = Crypto::new();
        crypto.derive_dek(password, &salt).expect("派生密钥失败");
        crypto
    }

    /// 利用提供的派生密钥解密给定数据
    pub fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>, CryptoError> {
        // 从前NONCE_LEN字节中提取nonce
        if encrypted.len() < NONCE_LEN {
            return Err(CryptoError::CiphertextTooShort);
        }
        let (nonce_bytes, ciphertext) = encrypted.split_at(NONCE_LEN);
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        let dek = guard.as_ref().ok_or(CryptoError::KeyNotDerived)?;
        let cipher =
            Aes256Gcm::new_from_slice(dek.as_ref()).map_err(|e| CryptoError::InvalidKey(format!("{:?}", e)))?;

        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| CryptoError::DecryptionFailed(format!("{:?}", e)))?;

        Ok(plaintext)
    }

    /// 使用CTR流式加密包装流
    pub fn encrypt_streaming(&self, stream: ByteStream) -> Result<(ByteStream, Vec<u8>), CryptoError> {
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        let dek = guard.as_ref().ok_or(CryptoError::KeyNotDerived)?;

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
    ) -> Result<ByteStream, CryptoError> {
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        let dek = guard.as_ref().ok_or(CryptoError::KeyNotDerived)?;

        if nonce.len() != CTR_NONCE_LEN {
            return Err(CryptoError::InvalidNonceLength { expected: CTR_NONCE_LEN, actual: nonce.len() });
        }

        let mut cipher = Aes256Ctr::new(dek.as_ref().into(), nonce.into());

        // 计算初始计数器值：start_offset / 16（块大小）
        cipher.seek(start_offset);

        let mapped_stream = ctr_stream_cipher(stream, cipher);

        Ok(mapped_stream)
    }

    /// 使用提供的派生密钥对给定数据进行加密。
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        let dek = guard.as_ref().ok_or(CryptoError::KeyNotDerived)?;
        let cipher =
            Aes256Gcm::new_from_slice(dek.as_ref()).map_err(|e| CryptoError::InvalidKey(format!("{:?}", e)))?;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| CryptoError::EncryptionFailed(format!("{:?}", e)))?;

        Ok([nonce_bytes.to_vec(), ciphertext].concat())
    }

    /// 从给定的密码和盐中推导出数据加密密钥（DEK）
    pub fn derive_dek(&self, password: String, salt: &str) -> Result<(), CryptoError> {
        let derived_key = derive_key(password, salt)?;

        // 使用写锁进行覆盖赋值
        if let Ok(mut guard) = self.inner.write() {
            *guard = Some(derived_key);
        }
        Ok(())
    }

    /// 验证密码获取加密密钥
    pub fn valid_password(&self, password: String, salt: &str) -> Result<String, CryptoError> {
        let derived_key = derive_key(password, salt)?;

        // 获取读锁，错误直接返回
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;

        // 取出已存储的密钥引用
        let stored_key = guard.as_ref().ok_or(CryptoError::NotInitialized)?;

        // 比较内部字节数组
        if derived_key.0 == stored_key.0 {
            Ok(hex::encode(derived_key.0))
        } else {
            Err(CryptoError::PasswordMismatch)
        }
    }

    /// 使用密钥字符串初始化
    pub fn init_by_dek_string(&self, dek: String) -> Result<(), CryptoError> {
        let dek_bytes: Vec<u8> =
            hex::decode(&dek).map_err(|e| CryptoError::InvalidDekHex(e.to_string()))?;
        if dek_bytes.len() != KEY_LEN {
            return Err(CryptoError::InvalidDekLength { expected: KEY_LEN, actual: dek_bytes.len() });
        }
        let mut dek_array = [0u8; KEY_LEN];
        dek_array.copy_from_slice(&dek_bytes);
        let derived_key = DerivedKey(dek_array);

        // 使用写锁进行覆盖赋值
        if let Ok(mut guard) = self.inner.write() {
            *guard = Some(derived_key);
        }

        dek_array.zeroize();
        Ok(())
    }
}

/// 派生密钥
fn derive_key(password: String, salt: &str) -> Result<DerivedKey, CryptoError> {
    let params = ParamsBuilder::new()
        .t_cost(2)
        .m_cost(MEMORY_COST_KIB)
        .p_cost(4)
        .output_len(KEY_LEN)
        .build()
        .map_err(|e| CryptoError::DeriveFailed(e.to_string()))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let salt = SaltString::from_b64(salt).map_err(|e| CryptoError::InvalidSalt(e.to_string()))?;
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| CryptoError::DeriveFailed(e.to_string()))?;

    let dek = hash.hash.ok_or(CryptoError::DeriveFailed("无法提取哈希值".to_string()))?;
    if dek.as_bytes().len() != KEY_LEN {
        return Err(CryptoError::DeriveFailed("派生的密钥长度不正确".to_string()));
    }

    let mut dek_array = [0u8; KEY_LEN];
    dek_array.copy_from_slice(dek.as_bytes());
    Ok(DerivedKey(dek_array))
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
