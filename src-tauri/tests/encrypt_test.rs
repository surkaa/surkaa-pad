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
}
