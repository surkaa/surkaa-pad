#[cfg(test)]
mod encrypt_test {
    use surkaa_pad_lib::encryption_manager::EncryptionManager;
    use tokio;

    #[tokio::test]
    async fn test_encrypt_decrypt() {
        let plaintext = b"Hello, Surkaa Pad!";
        let password = "strong_password";
        let salt = "dGVzdF9zYWx0";

        // 初始化加密管理器并派生密钥
        let manager = EncryptionManager::new();
        manager
            .initial(password, salt)
            .await
            .expect("Key derivation failed");

        // 加密
        let (encrypted_bytes, nonce_bytes) =
            manager.encrypt(plaintext).await.expect("Encryption failed");

        // 解密
        let decrypted_data = manager
            .decrypt(&encrypted_bytes, &nonce_bytes)
            .await
            .expect("Decryption failed");

        assert_eq!(plaintext.to_vec(), decrypted_data);
    }

    #[tokio::test]
    async fn test_decrypt_ture_data() {
        let true_data = vec![
            // TODO: 填入实际的加密数据和 nonce
        ];
        let (encrypted_bytes, nonce_bytes) = true_data.split_at(true_data.len() - 12);
        let password = "ture_data";
        let salt = "NFI2cXl3cUpiSDk4bVVkdEY4cDMzRzlqcTdMMkY5WDg";

        // 初始化加密管理器并派生密钥
        let manager = EncryptionManager::new();
        manager
            .initial(password, salt)
            .await
            .expect("Key derivation failed");

        // 解密
        let decrypted_data = manager
            .decrypt(&encrypted_bytes, &nonce_bytes)
            .await
            .expect("Decryption failed");

        let decrypted_string = String::from_utf8(decrypted_data).expect("Invalid UTF-8 data");
        print!("Decrypted string: {}", decrypted_string);
    }
}
