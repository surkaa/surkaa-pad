#[cfg(test)]
mod ali_oss_tests {
    use std::env;
    use aliyun_oss_client::{Bucket, Client, EndPoint, Key, Object, Secret};
    use std::sync::Arc;
    use surkaa_pad_lib::oss_client_manager::OssClientManager;
    use tokio;

    #[tokio::test]
    async fn test_oss_manager_list_objects() {
        let oss = OssClientManager::default();

        let key = env::var("ALIYUN_KEY").expect("ALIYUN_KEY 环境变量未设置");
        let secret = env::var("ALIYUN_SECRET").expect("ALIYUN_SECRET 环境变量未设置");
        let bucket_name = env::var("ALIYUN_BUCKET_NAME").expect("ALIYUN_BUCKET_NAME 环境变量未设置");
        let endpoint = env::var("ALIYUN_ENDPOINT").expect("ALIYUN_ENDPOINT 环境变量未设置");

        oss.initialize(&key, &secret, &bucket_name, &endpoint)
            .await
            .expect("Failed to initialize OSS client");
        let objects = oss.list_objects("test").await.expect("Failed to list objects");
        println!("Objects: {:?}", objects);
    }

    fn init_oss_client(ep: EndPoint) -> Result<Arc<Client>, String> {
        let key = env::var("ALIYUN_KEY").expect("ALIYUN_KEY 环境变量未设置");
        let secret = env::var("ALIYUN_SECRET").expect("ALIYUN_SECRET 环境变量未设置");
        let bucket_name = env::var("ALIYUN_BUCKET_NAME").expect("ALIYUN_BUCKET_NAME 环境变量未设置");
        let key = Key::new(key);
        let secret = Secret::new(secret);

        let bucket = Bucket::new(bucket_name, ep);
        let mut  client = Client::new(key, secret);
        client.set_bucket(bucket);

        Ok(Arc::new(client))
    }

    #[tokio::test]
    async fn test_init_oss_client() {
        let endpoint = env::var("ALIYUN_ENDPOINT").expect("ALIYUN_ENDPOINT 环境变量未设置");

        let ep = EndPoint::new(&endpoint)
            .map_err(|e| format!("无效的 Endpoint: {}", e))
            .unwrap();
        let client = init_oss_client(ep.clone()).expect("Failed to initialize OSS client");

        let buckets = client.get_buckets(&ep).await.expect("AK/SK check failed");

        assert_eq!(buckets.len(), 1, "Expected one bucket");
    }

    #[tokio::test]
    async fn test_upload_and_download_and_delete() {
        let endpoint = env::var("ALIYUN_ENDPOINT").expect("ALIYUN_ENDPOINT 环境变量未设置");
        let ep = EndPoint::new(&endpoint)
            .map_err(|e| format!("无效的 Endpoint: {}", e))
            .unwrap();
        let client = init_oss_client(ep.clone()).expect("Failed to initialize OSS client");

        let data: Vec<u8> = (0..100).collect();

        let key = "test/object.bin";

        Object::new(key)
            .upload(data.clone(), &client)
            .await
            .map_err(|e| e.to_string())
            .expect("Upload failed");

        let downloaded_data = Object::new(key)
            .download(&client)
            .await
            .map_err(|e| e.to_string())
            .expect("Download failed");

        assert_eq!(downloaded_data.len(), 100, "Downloaded data length mismatch");
        assert_eq!(downloaded_data, data, "Downloaded data mismatch");

        Object::new(key)
            .delete(&client)
            .await
            .map_err(|e| e.to_string())
            .expect("Delete failed");

        // 获取test下的对象列表，确保删除成功
        let objects = Object::new(key);
        assert!(objects.in_dir(), "Object should not exist after deletion");
    }
}