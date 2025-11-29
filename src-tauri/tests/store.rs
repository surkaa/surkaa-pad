#[cfg(test)]
mod secure_store {
    use std::sync::Arc;
    use surkaa_pad_lib::oss_manager::OssClientManager;
    use surkaa_pad_lib::secure_store::SecureDiaryStore;
    use tokio;
    use surkaa_pad_lib::encryption::EncryptionManager;

    const KEY: &str = "";
    const SECRET: &str = "";
    const ENDPOINT: &str = "";
    const BUCKET_NAME: &str = "";

    #[tokio::test]
    async fn test_list_diaries() {
        // 初始化 OSS 客户端管理器
        let oss = OssClientManager::default();
        oss.initialize(KEY, SECRET, ENDPOINT, BUCKET_NAME)
            .await
            .expect("Failed to initialize OSS client");

        let client = Arc::new(oss);

        let encryption = EncryptionManager::new();

        let store = SecureDiaryStore::new(client, encryption);

        let diary_ids = store.list_diary_ids().await.expect("Failed to list diaries");

        println!("Diary IDs: {:?}", diary_ids);
    }
}
