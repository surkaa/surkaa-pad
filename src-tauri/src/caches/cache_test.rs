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
        let cache = LocalFileCache::new_test();
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
    async fn test_save_stream_success() {
        let lfc = LocalFileCache::new_test();
        let key = "stream-key";
        let full_data = b"Hello, world! This is a test.";
        let stream = create_mock_stream(full_data.to_vec(), 5);

        let (wrapped_stream, handle) = lfc.save(key, stream).await.unwrap();

        // 消费流并收集数据，验证与原始数据一致
        let collected = collect_data(wrapped_stream).await.unwrap();
        assert_eq!(collected, full_data);

        // 完成缓存
        handle.finalize().await.unwrap();

        // 验证缓存
        let md5 = lfc.get(key).await.expect("获取失败").unwrap();
        assert_eq!(md5, md5_hex(full_data));
    }

    #[tokio::test]
    async fn test_save_stream_error_abort() {
        let cache = LocalFileCache::new_test();
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
    async fn test_save_stream_finalize_after_error() {
        let cache = LocalFileCache::new_test();
        let key = "error-finalize-key";
        let stream = futures_util::stream::iter(vec![
            Ok(Bytes::from("partial")),
            Err(io::Error::new(io::ErrorKind::Other, "boom")),
        ]);
        let stream = Box::pin(stream) as ByteStream;

        let (wrapped_stream, handle) = cache.save(key, stream).await.unwrap();

        // 消费到错误
        let _ = collect_data(wrapped_stream).await;

        // 尝试 finalize，应该失败并清理临时文件
        let finalize_result = handle.finalize().await;
        assert!(finalize_result.is_err());
        assert!(cache.get(key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_key() {
        let cache = LocalFileCache::new_test();
        cache.delete("ghost").await; // 不应 panic
    }

    #[tokio::test]
    async fn test_concurrent_save() {
        use std::sync::Arc;
        let cache = Arc::new(LocalFileCache::new_test());
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
        let cache = LocalFileCache::new_test();
        let key = "nested/dir/structure/file";
        let data = b"nested data";
        let stream = create_mock_stream(data.to_vec(), 1024);

        let (wrapped_stream, handle) = cache.save(key, stream).await.unwrap();
        collect_data(wrapped_stream).await.unwrap();
        handle.finalize().await.unwrap();

        assert!(cache.get(key).await.unwrap().is_some());
    }
}