pub mod command;
pub mod types;

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
impl std::ops::Deref for types::DerivedKey {
    type Target = [u8; types::KEY_LEN];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct InnerCrypto {
    dek: OnceLock<types::DerivedKey>,
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
        if encrypted.len() < types::NONCE_LEN {
            return Err("密文长度不足以包含 nonce".to_string());
        }
        let (nonce_bytes, ciphertext) = encrypted.split_at(types::NONCE_LEN);
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
        let mut nonce = [0u8; types::CTR_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);

        let cipher = types::Aes256Ctr::new(dek.as_ref().into(), (&nonce).into());

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

        if nonce.len() != types::CTR_NONCE_LEN {
            return Err("NONCE 长度错误".to_string());
        }

        let mut cipher = types::Aes256Ctr::new(dek.as_ref().into(), nonce.into());

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

        let mut nonce_bytes = [0u8; types::NONCE_LEN];
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
            .m_cost(types::MEMORY_COST_KIB)
            .p_cost(4)
            .output_len(types::KEY_LEN)
            .build()
            .map_err(|e| format!("参数错误:{}", e))?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let salt = SaltString::from_b64(salt.as_str()).map_err(|e| format!("非法盐:{}", e))?;

        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| format!("派生失败:{}", e))?;

        password.zeroize();

        let dek = hash.hash.ok_or("无法提取哈希值".to_string())?;
        if dek.as_bytes().len() == types::KEY_LEN {
            let mut dek_array = [0u8; types::KEY_LEN];
            dek_array.copy_from_slice(dek.as_bytes());
            let derived_key = types::DerivedKey(dek_array);
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
        if dek_bytes.len() != types::KEY_LEN {
            return Err("Invalid DEK length".to_string());
        }
        let mut dek_array = [0u8; types::KEY_LEN];
        dek_array.copy_from_slice(&dek_bytes);
        let derived_key = types::DerivedKey(dek_array);
        let _ = self.inner.dek.set(derived_key);
        dek_array.zeroize();
        Ok(())
    }
}

fn ctr_stream_cipher(stream: ByteStream, mut cipher: types::Aes256Ctr) -> ByteStream {
    Box::pin(stream.map(move |result| match result {
        Ok(bytes) => {
            let mut buffer = bytes.to_vec();
            cipher.apply_keystream(&mut buffer);
            Ok(Bytes::from(buffer))
        }
        Err(e) => Err(e),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::create_mock_stream;

    #[test]
    fn test_derive_encrypt_decrypt() {
        use base64::Engine;
        let password = "my_secure_password".to_string();
        dbg!(&password);
        use base64::engine::general_purpose::STANDARD;
        let salt = STANDARD.encode("random_salt_value").replace("=", "");
        dbg!(&salt);

        let crypto = Crypto::new();

        // 计算派生密钥所需要的时间
        let start_time = std::time::Instant::now();
        let dek_string = crypto.derive_dek(password, salt).expect("派生密钥失败");
        let duration = start_time.elapsed();
        println!("派生 DEK 用时: {:?}, 密钥字符串: {}", duration, dek_string);

        let data = b"The quick brown fox jumps over the lazy dog".repeat(10000);
        dbg!(&data.len());
        let start_time = std::time::Instant::now();
        let encrypted = match crypto.encrypt(&data) {
            Ok(result) => result,
            Err(e) => {
                panic!("加密失败: {:?}", e);
            }
        };
        let duration = start_time.elapsed();
        println!("加密用时: {:?}", duration);
        dbg!(&encrypted.len());

        let start_time = std::time::Instant::now();
        let decrypted_data = crypto.decrypt(&encrypted).expect("无法解密");
        dbg!(&decrypted_data.len());
        assert_eq!(data.to_vec(), decrypted_data);
        let duration = start_time.elapsed();
        println!("解密用时: {:?}", duration);
    }

    #[test]
    fn test_encrypt_and_decrypt_big_data() {
        let crypto = Crypto::from_env();

        // 创建一个随机5MB的文件
        let mut random_data = vec![0u8; 5 * 1024 * 1024];
        OsRng.fill_bytes(&mut random_data);

        // 加密测试
        let encrypt_start = std::time::Instant::now();
        let encrypted_data = crypto.encrypt(&random_data).expect("无法加密大文件");
        let encrypt_duration = encrypt_start.elapsed();
        println!("加密大文件用时: {:?}", encrypt_duration);

        // 解密测试
        let decrypt_start = std::time::Instant::now();
        let decrypted_data = crypto
            .decrypt(&encrypted_data)
            .expect("无法解密大文件");
        let decrypt_duration = decrypt_start.elapsed();
        println!("解密大文件用时: {:?}", decrypt_duration);

        assert_eq!(random_data, decrypted_data);
    }

    #[tokio::test]
    async fn test_ctr_streaming_encrypt_decrypt() {
        let crypto = Crypto::from_env();

        // 1. 生成 1MB + 42 字节的随机测试数据（故意弄一个非整数倍长度，测试流的边界处理）
        let mut original_data = vec![0u8; 1024 * 1024 + 42];
        OsRng.fill_bytes(&mut original_data);

        // --- 流式加密阶段 ---
        let encrypt_start = std::time::Instant::now();

        // 模拟 64KB 为一个 Chunk 的数据流
        let input_stream = create_mock_stream(original_data.clone(), 64 * 1024);

        // 【注意】：这里假设你已经把名字改正确了，即 encrypt_streaming 是负责生成 nonce 的那个！
        let (mut encrypted_stream, nonce) = crypto
            .encrypt_streaming(input_stream)
            .expect("流式加密失败");

        // 消费流，把加密后的块重新收集到 Vec 中
        let mut encrypted_data = Vec::new();
        while let Some(chunk_result) = encrypted_stream.next().await {
            let chunk = chunk_result.expect("读取加密流失败");
            encrypted_data.extend_from_slice(&chunk);
        }

        let encrypt_duration = encrypt_start.elapsed();
        println!("CTR 流式加密用时: {:?}", encrypt_duration);

        // 验证：加密后的数据长度应与原数据一致（CTR 特性，0 字节膨胀），且内容不同
        assert_eq!(original_data.len(), encrypted_data.len());
        assert_ne!(original_data, encrypted_data);
        assert_eq!(nonce.len(), 16); // 验证 IV 长度必须是 16 字节

        // --- 流式解密阶段 ---
        let decrypt_start = std::time::Instant::now();

        // 将刚才收集的密文再次变成 Stream 喂给解密器
        let encrypted_input_stream = create_mock_stream(encrypted_data, 64 * 1024);
        let mut decrypted_stream = crypto
            .decrypt_streaming(encrypted_input_stream, &nonce, 0)
            .expect("流式解密失败");

        let mut decrypted_data = Vec::new();
        while let Some(chunk_result) = decrypted_stream.next().await {
            let chunk = chunk_result.expect("读取解密流失败");
            decrypted_data.extend_from_slice(&chunk);
        }

        let decrypt_duration = decrypt_start.elapsed();
        println!("CTR 流式解密用时: {:?}", decrypt_duration);

        // 终极验证：解密后的明文必须和最开始的原始数据一模一样
        assert_eq!(original_data, decrypted_data);
    }
}
