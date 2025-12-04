#[cfg(test)]
mod app_state_test {
    use std::env;
    use surkaa_pad_lib::encryption_manager::EncryptionManager;
    use surkaa_pad_lib::oss_client_manager::OssClientManager;
    use surkaa_pad_lib::secure_diary_store::SecureDiaryStore;
    use surkaa_pad_lib::surkaa_pad::{AppState, DiaryMemoryCache};
    use tokio;

    async fn create_store() -> (EncryptionManager, OssClientManager, SecureDiaryStore) {
        // 初始化 OSS 客户端管理器
        let oss = OssClientManager::default();
        dotenvy::dotenv().ok();
        let key = env::var("ALIYUN_KEY").expect("ALIYUN_KEY 环境变量未设置");
        let secret = env::var("ALIYUN_SECRET").expect("ALIYUN_SECRET 环境变量未设置");
        let bucket_name =
            env::var("ALIYUN_BUCKET_NAME").expect("ALIYUN_BUCKET_NAME 环境变量未设置");
        let endpoint = env::var("ALIYUN_ENDPOINT").expect("ALIYUN_ENDPOINT 环境变量未设置");
        let mp = env::var("MASTER_PASSWORD").unwrap();
        let salt = env::var("SALT").unwrap();

        oss.initialize(&key, &secret, &endpoint, &bucket_name)
            .await
            .expect("Failed to initialize OSS client");

        let encryption = EncryptionManager::new();

        encryption
            .initial(&mp, &salt)
            .await
            .expect("Failed to initialize encryption manager");

        (encryption, oss, SecureDiaryStore::new())
    }

    #[test]
    fn test_app_path() {
        let state = AppState {};
        let path_buf = state.get_diary_cache_dir(None);
        println!("path_buf: {:?}", path_buf);
        assert!(path_buf.ends_with("diary_cache"));
    }

    #[tokio::test]
    async fn test_sync() {
        let state = AppState {};
        let (encryption, oss, store) = create_store().await;
        let cache = DiaryMemoryCache::new();

        state
            .load_cache_to_memory(&cache, &encryption, &store, None)
            .await
            .expect("Failed to load cache");

        let cur_diaries = state.list_cached_diaries(&cache).await;
        println!("Current cached diaries: {:?}", cur_diaries);
        assert!(cur_diaries.is_empty());

        state
            .sync_from_oss(&cache, &encryption, &oss, &store, None)
            .await
            .expect("Failed to sync from OSS");

        state
            .load_cache_to_memory(&cache, &encryption, &store, None)
            .await
            .expect("Failed to load cache");

        let updated_diaries = state.list_cached_diaries(&cache).await;
        println!("Updated cached diaries: {:?}", updated_diaries);
        assert!(!updated_diaries.is_empty());
    }

    #[tokio::test]
    async fn test_decrypt_enc_file() {
        let state = AppState {};
        let (encryption, _, _) = create_store().await;

        // 获取本地加密文件路径
        let enc_file_path = state.get_diary_cache_dir(None);
        // 读取enc_file_path下的所有加密文件
        let entries =
            std::fs::read_dir(&enc_file_path).expect("Failed to read diary cache directory");
        for entry in entries {
            let entry = entry.expect("Failed to get directory entry");
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("enc") {
                // 读取得到字节流
                let enc_data = std::fs::read(&path).expect("Failed to read encrypted file");
                // 划分nonce和密文
                let (nonce, enc_data) = enc_data.split_at(12);
                println!("读取到加密文件: {:?}", path);
                println!("nonce: {:?}", nonce);
                println!("enc_data.len: {:?}", enc_data.len());
                // 解密字节流
                let decrypted_data = encryption
                    .decrypt(&enc_data, nonce)
                    .await
                    .expect("Failed to decrypt data");
                println!(
                    "decrypted_data: {:?}",
                    String::from_utf8_lossy(&decrypted_data)
                );
            }
        }
    }
}
