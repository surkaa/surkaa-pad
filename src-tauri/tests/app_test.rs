#[cfg(test)]
mod app_test {
    use std::sync::Arc;
    use surkaa_pad_lib::encryption_manager::EncryptionManager;
    use surkaa_pad_lib::oss_client_manager::OssClientManager;
    use surkaa_pad_lib::secure_diary_store::SecureDiaryStore;
    use surkaa_pad_lib::surkaa_pad::AppState;

    const KEY: &str = "";
    const SECRET: &str = "";
    const ENDPOINT: &str = "";
    const BUCKET_NAME: &str = "";

    async fn create_test_app_state() -> AppState {
        let oss = OssClientManager::default();
        oss.initialize(KEY, SECRET, ENDPOINT, BUCKET_NAME)
            .await
            .expect("Failed to initialize OSS client");

        let client = Arc::new(oss);
        let mut encryption = EncryptionManager::new();
        encryption
            .initial("strong_password", "dGVzdF9zYWx0")
            .expect("Failed to initialize encryption manager");

        let store = SecureDiaryStore::new(client, encryption);
        AppState::new(store)
    }

    #[tokio::test]
    async fn test_get_cache() {
        let state = create_test_app_state().await;
        assert_eq!(
            state.get_diary_cache_dir().to_str().unwrap(),
            r"C:\Users\SurKaa\AppData\Roaming\cn.surkaa.pad\diary_cache",
            "Diary cache directory path is incorrect"
        );
    }

    #[tokio::test]
    async fn test_app_state_sync_diary() {
        let state = create_test_app_state().await;
        let result = state.sync_from_oss().await;
        assert!(result.is_ok(), "Failed to sync diary: {:?}", result.err());

        // 获取内存的日记
        let diaries = state.list_cached_diaries();
        assert!(
            !diaries.is_empty(),
            "No diaries found in memory cache after sync"
        );

        // 查看本地缓存目录
        let cache_dir = state.get_diary_cache_dir();
        let entries = std::fs::read_dir(&cache_dir)
            .expect("Failed to read cache directory");
        let mut found = false;
        for entry in entries {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("enc") {
                found = true;
                break;
            }
        }
        assert!(found, "No .enc files found in cache directory after sync");
    }
}
