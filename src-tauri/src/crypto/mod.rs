use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::aes::cipher::crypto_common::rand_core::RngCore;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, ParamsBuilder, PasswordHasher, Version};
use std::sync::{Arc, OnceLock};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// 定义常量
const NONCE_LEN: usize = 12;
/// 定义派生密钥的长度（字节），AES-256 需要 32 字节
const KEY_LEN: usize = 32;
/// 定义内存成本（KiB）
#[cfg(debug_assertions)]
const MEMORY_COST_KIB: u32 = 1024;
#[cfg(not(debug_assertions))]
const MEMORY_COST_KIB: u32 = 256 * 1024;

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct DerivedKey(pub [u8; KEY_LEN]);

impl std::ops::Deref for DerivedKey {
    type Target = [u8; KEY_LEN];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub struct Crypto {
    dek: Arc<OnceLock<DerivedKey>>,
    algorithm: String,
}

impl Crypto {
    pub fn new() -> Self {
        Crypto {
            dek: Arc::new(OnceLock::new()),
            algorithm: "AES256-GCM_v1".to_string(),
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

    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    /// 利用提供的派生密钥解密给定数据
    pub fn decrypt(&self, ciphertext: &[u8], nonce_bytes: &[u8]) -> Result<Vec<u8>, String> {
        // 从前NONCE_LEN字节中提取nonce
        let dek = self.dek.get().ok_or("未派生密钥".to_string())?;
        let cipher =
            Aes256Gcm::new_from_slice(dek.as_ref()).map_err(|e| format!("无效的密钥: {:?}", e))?;

        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("解密失败: {:?}", e))?;

        Ok(plaintext)
    }

    /// 解密
    pub fn decrypt_from_full_ciphertext(&self, full_ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        if full_ciphertext.len() < NONCE_LEN {
            return Err("密文长度不足以包含 nonce".to_string());
        }
        let (nonce_bytes, ciphertext) = full_ciphertext.split_at(NONCE_LEN);
        self.decrypt(ciphertext, nonce_bytes)
    }

    /// 使用提供的派生密钥对给定数据进行加密。
    pub fn encrypt(&self, data: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
        let dek = self.dek.get().ok_or("未派生密钥".to_string())?;
        let cipher =
            Aes256Gcm::new_from_slice(dek.as_ref()).map_err(|e| format!("无效的密钥: {:?}", e))?;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| format!("加密失败: {:?}", e))?;

        Ok((ciphertext, nonce_bytes.to_vec()))
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
            #[cfg(debug_assertions)]
            {
                // 将派生的密钥打印到调试控制台
                use base64::Engine;
                let dek_b64 = base64::engine::general_purpose::STANDARD.encode(&dek_array);
                println!("派生的 DEK (Base64): {}", dek_b64);
            }
            let derived_key = DerivedKey(dek_array);
            let _ = self.dek.set(derived_key);
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
        let _ = self.dek.set(derived_key);
        dek_array.zeroize();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (encrypted, nonce) = match crypto.encrypt(&data) {
            Ok(result) => result,
            Err(e) => {
                panic!("加密失败: {:?}", e);
            }
        };
        let duration = start_time.elapsed();
        println!("加密用时: {:?}", duration);
        dbg!(&encrypted.len());

        let start_time = std::time::Instant::now();
        let decrypted_data = crypto.decrypt(&encrypted, &nonce).expect("无法解密");
        dbg!(&decrypted_data.len());
        assert_eq!(data.to_vec(), decrypted_data);
        let duration = start_time.elapsed();
        println!("解密用时: {:?}", duration);
    }

    #[test]
    fn convert_enc2normal() {
        use crate::diary::DiaryManifest;
        dotenvy::dotenv().ok();
        let enc_dir = std::env::var("TEST_ENC_DIR").expect("TEST_ENC_DIR 未设置");
        let output_dir = std::env::var("TEST_OUTPUT_DIR").expect("TEST_OUTPUT_DIR 未设置");
        let crypto = Crypto::from_env();
        let diary_filename = "manifest.enc";

        let enc_path = std::path::Path::new(&enc_dir);

        for diary_dir in std::fs::read_dir(enc_path).expect("无法读取目录") {
            let diary_dir = diary_dir.expect("无法读取条目");
            let diary_dir_path = diary_dir.path();
            if diary_dir_path.is_file() {
                continue;
            }
            let output_path = std::path::Path::new(&output_dir).join(diary_dir.file_name());
            std::fs::create_dir_all(&output_path).expect("无法创建输出目录");

            let manifest_path = diary_dir_path.join(diary_filename);
            let encrypted_manifest =
                std::fs::read(&manifest_path).expect("无法读取manifest.enc文件");
            let decrypted_manifest = crypto
                .decrypt_from_full_ciphertext(&encrypted_manifest)
                .expect("无法解密manifest文件");
            let manifest: DiaryManifest =
                serde_json::from_slice(&decrypted_manifest).expect("无法解析DiaryManifest JSON");

            let mut content = manifest.content.clone();
            let mut filename_mapping = std::collections::HashMap::new();

            for attachment in &manifest.attachments {
                let encrypted_attachment_path = diary_dir_path.join(&attachment.filename);
                let encrypted_data = std::fs::read(&encrypted_attachment_path)
                    .expect(&format!("无法读取附件文件: {}", attachment.filename));

                let decrypted_data = crypto
                    .decrypt(&encrypted_data, &attachment.nonce)
                    .expect(&format!("无法解密附件: {}", attachment.filename));

                let new_extension = if content.contains(&format!("<IMG:{}>", attachment.filename)) {
                    "jpg"
                } else if content.contains(&format!("<AUD:{}>", attachment.filename)) {
                    "mp3"
                } else if content.contains(&format!("<VID:{}>", attachment.filename)) {
                    "mp4"
                } else {
                    continue;
                };

                let stem = std::path::Path::new(&attachment.filename)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&attachment.filename);
                let new_filename = format!("{}.{}", stem, new_extension);
                filename_mapping.insert(attachment.filename.clone(), new_filename.clone());

                let output_attachment_path = output_path.join(new_filename);
                std::fs::write(&output_attachment_path, decrypted_data)
                    .expect(&format!("无法写入附件文件: {}", attachment.filename));
            }

            for (old_filename, new_filename) in filename_mapping {
                let tag_pattern = format!(
                    "<{}:{}>",
                    if new_filename.ends_with(".jpg") {
                        "IMG"
                    } else if new_filename.ends_with(".mp3") {
                        "AUD"
                    } else {
                        "VID"
                    },
                    old_filename
                );
                content = content.replace(&tag_pattern, &new_filename);
            }

            let diary_content_path = output_path.join("diary.txt");
            std::fs::write(&diary_content_path, content).expect("无法写入日记内容");

            let manifest_json_path = output_path.join("manifest.json");
            let manifest_json =
                serde_json::to_string_pretty(&manifest).expect("无法序列化manifest");
            std::fs::write(&manifest_json_path, manifest_json).expect("无法写入manifest.json");

            println!("解析完成: {}", output_path.display());
        }
    }

    #[test]
    fn test_encrypt_and_decrypt_big_data() {
        let crypto = Crypto::from_env();

        // 创建一个随机5MB的文件
        let mut random_data = vec![0u8; 5 * 1024 * 1024];
        OsRng.fill_bytes(&mut random_data);

        // 加密测试
        let encrypt_start = std::time::Instant::now();
        let (encrypted_data, nonce) = crypto.encrypt(&random_data).expect("无法加密大文件");
        let encrypt_duration = encrypt_start.elapsed();
        println!("加密大文件用时: {:?}", encrypt_duration);

        // 解密测试
        let decrypt_start = std::time::Instant::now();
        let decrypted_data = crypto
            .decrypt(&encrypted_data, &nonce)
            .expect("无法解密大文件");
        let decrypt_duration = decrypt_start.elapsed();
        println!("解密大文件用时: {:?}", decrypt_duration);

        assert_eq!(random_data, decrypted_data);
    }
}
