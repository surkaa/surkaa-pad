#[cfg(test)]
mod lfc_tests {
    use crate::caches::LocalFileCache;
    use crate::stream::{collect_data, create_mock_stream, ByteStream};
    use bytes::Bytes;
    use std::io;

    // 辅助函数：计算字节切片的 MD5 大写十六进制
    fn md5_hex(data: &[u8]) -> String {
        format!("{:X}", md5::compute(data))
    }

    #[tokio::test]
    async fn test_save_bytes_and_get() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let cache = LocalFileCache::new(path);
        let key = "test-key";
        let data = b"hello world";

        cache.save_bytes(key, data).await.unwrap();

        let md5 = cache.get(key).await.unwrap().unwrap();
        assert_eq!(md5, md5_hex(data));

        let retrieved = cache.get_data(key).await.unwrap();
        assert_eq!(retrieved, data);

        // 测试 get_stream
        let stream = cache.get_stream(key, None).await.unwrap();
        let collected = collect_data(stream).await.unwrap();
        assert_eq!(collected, data);

        cache.delete(key).await;
        assert!(cache.get(key).await.unwrap().is_none());
        assert!(cache.get_data(key).await.is_err());
    }

    #[tokio::test]
    async fn test_save_stream_error_abort() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let cache = LocalFileCache::new(path);
        let key = "abort-key";
        // 创建一个会出错的流：第一个块正常，第二个块返回错误
        let stream = futures_util::stream::iter(vec![
            Ok(Bytes::from("good data")),
            Err(io::Error::new(io::ErrorKind::Other, "simulated error")),
        ]);
        // 转换为 ByteStream
        let stream = Box::pin(stream) as ByteStream;

        let (wrapped_stream, handle) = cache.save(key, stream).await.unwrap();

        // 消费流，遇到错误后循环退出
        let result = collect_data(wrapped_stream).await;
        assert!(result.is_err());

        // 放弃缓存
        handle.abort().await;

        // 验证没有生成缓存文件
        assert!(cache.get(key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_key() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let cache = LocalFileCache::new(path);
        cache.delete("ghost").await;
        assert!(cache.get("ghost").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_concurrent_save() {
        use std::sync::Arc;
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let cache = Arc::new(LocalFileCache::new(path));
        let key1 = "key1";
        let key2 = "key2";
        let data1 = b"data1";
        let data2 = b"data2";

        let cache1 = cache.clone();
        let task1 = tokio::spawn(async move {
            cache1.save_bytes(key1, data1).await.unwrap();
        });

        let cache2 = cache.clone();
        let task2 = tokio::spawn(async move {
            cache2.save_bytes(key2, data2).await.unwrap();
        });

        task1.await.unwrap();
        task2.await.unwrap();

        assert!(cache.get(key1).await.unwrap().is_some());
        assert!(cache.get(key2).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_save_stream_parent_dir_creation() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let cache = LocalFileCache::new(path);
        let key = "nested/dir/structure/file";
        let data = b"nested data";
        let md5 = format!("{:X}", md5::compute(data));
        let stream = create_mock_stream(data.to_vec(), 1024);

        let (wrapped_stream, handle) = cache.save(key, stream).await.unwrap();
        collect_data(wrapped_stream).await.unwrap();
        handle.finalize(md5).await.unwrap();

        assert!(cache.get(key).await.unwrap().is_some());
    }
}
