#[cfg(test)]
mod tests {
    use crate::cryptos::Crypto;
    use crate::utils::create_mock_stream;
    use aes_gcm::aead::OsRng;
    use aes_gcm::aes::cipher::crypto_common::rand_core::RngCore;
    use futures_util::StreamExt;

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
        let decrypted_data = crypto.decrypt(&encrypted_data).expect("无法解密大文件");
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
