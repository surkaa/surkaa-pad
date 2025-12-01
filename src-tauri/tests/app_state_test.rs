#[cfg(test)]
mod secure_store {
    use std::env;
    use surkaa_pad_lib::encryption_manager::EncryptionManager;
    use surkaa_pad_lib::oss_client_manager::OssClientManager;
    use surkaa_pad_lib::secure_diary_store::SecureDiaryStore;
    use tokio;

    async fn create_store() -> (EncryptionManager, OssClientManager, SecureDiaryStore) {
        // 初始化 OSS 客户端管理器
        let oss = OssClientManager::default();
        dotenvy::dotenv().ok();
        let key = env::var("ALIYUN_KEY").expect("ALIYUN_KEY 环境变量未设置");
        let secret = env::var("ALIYUN_SECRET").expect("ALIYUN_SECRET 环境变量未设置");
        let bucket_name = env::var("ALIYUN_BUCKET_NAME").expect("ALIYUN_BUCKET_NAME 环境变量未设置");
        let endpoint = env::var("ALIYUN_ENDPOINT").expect("ALIYUN_ENDPOINT 环境变量未设置");

        oss.initialize(&key, &secret, &endpoint, &bucket_name)
            .await
            .expect("Failed to initialize OSS client");

        let encryption = EncryptionManager::new();

        encryption
            .initial("strong_password", "dGVzdF9zYWx0")
            .await
            .expect("Failed to initialize encryption manager");

        (encryption, oss, SecureDiaryStore {})
    }
}