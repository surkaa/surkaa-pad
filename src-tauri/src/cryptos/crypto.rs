use super::crypto_types::{Aes256Ctr, DerivedKey, CTR_NONCE_LEN, KEY_LEN, NONCE_LEN};
use crate::cryptos::CryptoError;
use crate::stream::ByteStream;
use crate::vault_bootstrap::{
    KeyDerivationAlgorithm, KeyDerivationParameters, VaultBootstrap, VaultBootstrapError,
    ARGON2_VERSION_13, VAULT_VERIFIER_TEXT,
};
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
    inner: Arc<RwLock<Option<CryptoState>>>,
}

struct CryptoState {
    key: DerivedKey,
    kdf: KeyDerivationParameters,
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
        let state = guard.as_ref().ok_or(CryptoError::KeyNotDerived)?;
        let cipher = Aes256Gcm::new_from_slice(state.key.as_ref())
            .map_err(|e| CryptoError::InvalidKey(format!("{:?}", e)))?;

        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| CryptoError::DecryptionFailed(format!("{:?}", e)))?;

        Ok(plaintext)
    }

    /// 使用CTR流式加密包装流
    pub fn encrypt_streaming(
        &self,
        stream: ByteStream,
    ) -> Result<(ByteStream, Vec<u8>), CryptoError> {
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        let state = guard.as_ref().ok_or(CryptoError::KeyNotDerived)?;

        // 生成随机NONCE
        let mut nonce = [0u8; CTR_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);

        let cipher = Aes256Ctr::new(state.key.as_ref().into(), (&nonce).into());

        let mapped_stream = ctr_stream_cipher(stream, cipher);

        Ok((mapped_stream, nonce.to_vec()))
    }

    /// 创建独立的 CTR cipher 实例，用于分片加密
    /// 返回 (cipher, nonce_bytes)，调用方通过 cipher.apply_keystream 逐块加密
    pub fn create_ctr_cipher(&self) -> Result<(Aes256Ctr, Vec<u8>), CryptoError> {
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        let state = guard.as_ref().ok_or(CryptoError::KeyNotDerived)?;

        let mut nonce = [0u8; CTR_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);

        let cipher = Aes256Ctr::new(state.key.as_ref().into(), (&nonce).into());
        Ok((cipher, nonce.to_vec()))
    }

    /// 使用CTR流式解密
    pub fn decrypt_streaming(
        &self,
        stream: ByteStream,
        nonce: &[u8],
        start_offset: u64,
    ) -> Result<ByteStream, CryptoError> {
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        let state = guard.as_ref().ok_or(CryptoError::KeyNotDerived)?;

        if nonce.len() != CTR_NONCE_LEN {
            return Err(CryptoError::InvalidNonceLength {
                expected: CTR_NONCE_LEN,
                actual: nonce.len(),
            });
        }

        let mut cipher = Aes256Ctr::new(state.key.as_ref().into(), nonce.into());

        // 计算初始计数器值：start_offset / 16（块大小）
        cipher.seek(start_offset);

        let mapped_stream = ctr_stream_cipher(stream, cipher);

        Ok(mapped_stream)
    }

    /// 使用提供的派生密钥对给定数据进行加密。
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        let state = guard.as_ref().ok_or(CryptoError::KeyNotDerived)?;
        let cipher = Aes256Gcm::new_from_slice(state.key.as_ref())
            .map_err(|e| CryptoError::InvalidKey(format!("{:?}", e)))?;

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
        let mut parameters = KeyDerivationParameters::legacy_current();
        parameters.salt = salt.to_owned();
        self.derive_dek_with_parameters(password, parameters)
    }

    pub fn derive_dek_with_parameters(
        &self,
        password: String,
        parameters: KeyDerivationParameters,
    ) -> Result<(), CryptoError> {
        let derived_key = derive_key(password, &parameters)?;
        self.set_derived_key(derived_key, parameters)
    }

    pub fn derive_and_verify_bootstrap(
        &self,
        password: String,
        bootstrap: &VaultBootstrap,
    ) -> Result<(), VaultBootstrapError> {
        bootstrap.validate()?;
        let derived_key = derive_key(password, &bootstrap.kdf)
            .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))?;
        let verifier = bootstrap.decode_verifier()?;
        let plaintext = decrypt_with_key(&derived_key, &verifier)
            .map_err(|_| VaultBootstrapError::VerifierMismatch)?;
        if plaintext != VAULT_VERIFIER_TEXT.as_bytes() {
            return Err(VaultBootstrapError::VerifierMismatch);
        }
        self.set_derived_key(derived_key, bootstrap.kdf.clone())
            .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))
    }

    pub fn derive_and_verify_ciphertext(
        &self,
        password: String,
        parameters: KeyDerivationParameters,
        encrypted_probe: &[u8],
    ) -> Result<(), CryptoError> {
        let derived_key = derive_key(password, &parameters)?;
        decrypt_with_key(&derived_key, encrypted_probe)?;
        self.set_derived_key(derived_key, parameters)
    }

    pub fn active_kdf_parameters(&self) -> Result<KeyDerivationParameters, CryptoError> {
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        Ok(guard
            .as_ref()
            .ok_or(CryptoError::KeyNotDerived)?
            .kdf
            .clone())
    }

    pub fn is_initialized(&self) -> Result<bool, CryptoError> {
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        Ok(guard.is_some())
    }

    pub fn candidate_matches_active_key(
        &self,
        password: String,
        parameters: &KeyDerivationParameters,
    ) -> Result<bool, CryptoError> {
        let candidate = derive_key(password, parameters)?;
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;
        let active = guard.as_ref().ok_or(CryptoError::NotInitialized)?;
        Ok(candidate.0 == active.key.0)
    }

    pub fn validate_bootstrap_for_active_key(
        &self,
        password: String,
        bootstrap: &VaultBootstrap,
    ) -> Result<bool, VaultBootstrapError> {
        bootstrap.validate()?;
        let candidate = derive_key(password, &bootstrap.kdf)
            .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))?;
        let verifier = bootstrap.decode_verifier()?;
        let plaintext = decrypt_with_key(&candidate, &verifier)
            .map_err(|_| VaultBootstrapError::VerifierMismatch)?;
        if plaintext != VAULT_VERIFIER_TEXT.as_bytes() {
            return Err(VaultBootstrapError::VerifierMismatch);
        }
        let guard = self
            .inner
            .read()
            .map_err(|_| VaultBootstrapError::Storage("加密状态锁已损坏".into()))?;
        let active = guard.as_ref().ok_or(VaultBootstrapError::NotInitialized)?;
        Ok(candidate.0 == active.key.0)
    }

    fn set_derived_key(
        &self,
        derived_key: DerivedKey,
        parameters: KeyDerivationParameters,
    ) -> Result<(), CryptoError> {
        let mut guard = self.inner.write().map_err(|_| CryptoError::LockPoisoned)?;
        *guard = Some(CryptoState {
            key: derived_key,
            kdf: parameters,
        });
        Ok(())
    }

    /// 验证密码获取加密密钥
    pub fn valid_password(&self, password: String) -> Result<String, CryptoError> {
        let parameters = self.active_kdf_parameters()?;
        let derived_key = derive_key(password, &parameters)?;

        // 获取读锁，错误直接返回
        let guard = self.inner.read().map_err(|_| CryptoError::LockPoisoned)?;

        // 取出已存储的密钥引用
        let stored_key = &guard.as_ref().ok_or(CryptoError::NotInitialized)?.key;

        // 比较内部字节数组
        if derived_key.0 == stored_key.0 {
            Ok(hex::encode(derived_key.0))
        } else {
            Err(CryptoError::PasswordMismatch)
        }
    }

    /// 使用密钥字符串初始化
    pub fn init_by_dek_string(&self, dek: String) -> Result<(), CryptoError> {
        self.init_by_dek_string_with_parameters(dek, KeyDerivationParameters::legacy_current())
    }

    pub fn init_by_dek_string_with_parameters(
        &self,
        dek: String,
        parameters: KeyDerivationParameters,
    ) -> Result<(), CryptoError> {
        let dek_bytes: Vec<u8> =
            hex::decode(&dek).map_err(|e| CryptoError::InvalidDekHex(e.to_string()))?;
        if dek_bytes.len() != KEY_LEN {
            return Err(CryptoError::InvalidDekLength {
                expected: KEY_LEN,
                actual: dek_bytes.len(),
            });
        }
        let mut dek_array = [0u8; KEY_LEN];
        dek_array.copy_from_slice(&dek_bytes);
        let derived_key = DerivedKey(dek_array);

        self.set_derived_key(derived_key, parameters)?;

        dek_array.zeroize();
        Ok(())
    }
}

/// 派生密钥
pub(crate) fn derive_key(
    password: String,
    parameters: &KeyDerivationParameters,
) -> Result<DerivedKey, CryptoError> {
    parameters
        .validate()
        .map_err(|error| CryptoError::DeriveFailed(error.to_string()))?;
    let params = ParamsBuilder::new()
        .t_cost(parameters.time_cost)
        .m_cost(parameters.memory_cost_kib)
        .p_cost(parameters.parallelism)
        .output_len(parameters.output_length as usize)
        .build()
        .map_err(|e| CryptoError::DeriveFailed(e.to_string()))?;

    let algorithm = match parameters.algorithm {
        KeyDerivationAlgorithm::Argon2id => Algorithm::Argon2id,
    };
    let version = match parameters.algorithm_version {
        ARGON2_VERSION_13 => Version::V0x13,
        version => {
            return Err(CryptoError::DeriveFailed(format!(
                "不支持的 Argon2 版本: {version}"
            )))
        }
    };
    let argon2 = Argon2::new(algorithm, version, params);
    let salt = SaltString::from_b64(&parameters.salt)
        .map_err(|e| CryptoError::InvalidSalt(e.to_string()))?;
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| CryptoError::DeriveFailed(e.to_string()))?;

    let dek = hash
        .hash
        .ok_or(CryptoError::DeriveFailed("无法提取哈希值".to_string()))?;
    if dek.as_bytes().len() != KEY_LEN {
        return Err(CryptoError::DeriveFailed(
            "派生的密钥长度不正确".to_string(),
        ));
    }

    let mut dek_array = [0u8; KEY_LEN];
    dek_array.copy_from_slice(dek.as_bytes());
    Ok(DerivedKey(dek_array))
}

fn decrypt_with_key(key: &DerivedKey, encrypted: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if encrypted.len() < NONCE_LEN {
        return Err(CryptoError::CiphertextTooShort);
    }
    let (nonce_bytes, ciphertext) = encrypted.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|error| CryptoError::InvalidKey(format!("{error:?}")))?;
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|error| CryptoError::DecryptionFailed(format!("{error:?}")))
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
