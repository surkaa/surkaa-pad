#[cfg(test)]
mod ali_oss_tests {
    use std::sync::Arc;
    use tokio;
    use aliyun_oss_client::{Bucket, Client, EndPoint, Key, Object, Secret};

    const KEY: &str = "";
    const SECRET: &str = "";
    const BUCKET_NAME: &str = "";
    const ENDPOINT: &str = "cn-guangzhou";

    fn init_oss_client(ep: EndPoint) -> Result<Arc<Client>, String> {
        let key = Key::new(KEY);
        let secret = Secret::new(SECRET);

        let bucket = Bucket::new(BUCKET_NAME, ep);
        let mut  client = Client::new(key, secret);
        client.set_bucket(bucket);

        Ok(Arc::new(client))
    }

    #[tokio::test]
    async fn test_init_oss_client() {
        let ep = EndPoint::new(ENDPOINT)
            .map_err(|e| format!("无效的 Endpoint: {}", e))
            .unwrap();
        let client = init_oss_client(ep.clone()).expect("Failed to initialize OSS client");

        let buckets = client.get_buckets(&ep).await.expect("AK/SK check failed");

        assert_eq!(buckets.len(), 1, "Expected one bucket");
    }

    #[tokio::test]
    async fn test_upload_and_download_and_delete() {
        let ep = EndPoint::new(ENDPOINT)
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