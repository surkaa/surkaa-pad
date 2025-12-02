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

        (encryption, oss, SecureDiaryStore {})
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

        state.load_cache_to_memory(&cache, &encryption, &store, None).await
            .expect("Failed to load cache");

        let cur_diaries = state.list_cached_diaries(&cache).await;
        println!("Current cached diaries: {:?}", cur_diaries);
        assert!(cur_diaries.is_empty());

        state
            .sync_from_oss(&cache, &encryption, &oss, &store, None)
            .await
            .expect("Failed to sync from OSS");

        state.load_cache_to_memory(&cache, &encryption, &store, None).await
            .expect("Failed to load cache");

        let updated_diaries = state.list_cached_diaries(&cache).await;
        println!("Updated cached diaries: {:?}", updated_diaries);
        assert!(!updated_diaries.is_empty());
    }
}
