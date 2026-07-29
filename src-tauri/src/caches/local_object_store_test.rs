#[cfg(test)]
mod los_tests {
    use crate::caches::LocalObjectStore;
    use crate::stream::{collect_data, create_mock_stream};
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
        let cache = LocalObjectStore::new(path);
        let key = "test-key";
        let data = b"hello world";

        cache.save_bytes(key, data).await.unwrap();

        let md5 = cache.get(key).await.unwrap().unwrap();
        assert_eq!(md5, md5_hex(data));
        assert_eq!(cache.get_size(key).await.unwrap(), Some(data.len() as u64));

        let retrieved = cache.get_data(key).await.unwrap();
        assert_eq!(retrieved, data);

        // 测试 get_stream
        let stream = cache.get_stream(key, None).await.unwrap();
        let collected = collect_data(stream).await.unwrap();
        assert_eq!(collected, data);

        cache.delete(key).await.unwrap();
        assert!(cache.get(key).await.unwrap().is_none());
        assert!(cache.get_size(key).await.unwrap().is_none());
        assert!(cache.get_data(key).await.is_err());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_key() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let cache = LocalObjectStore::new(path);
        cache.delete("ghost").await.unwrap();
        assert!(cache.get("ghost").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_concurrent_save() {
        use std::sync::Arc;
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let cache = Arc::new(LocalObjectStore::new(path));
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
    async fn test_save_stream_with_etag_replaces_cache_after_complete_stream() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let cache = LocalObjectStore::new(temp_dir.path().to_path_buf());
        let key = "nested/streamed-file";
        cache.save_bytes(key, b"old data").await.unwrap();

        cache
            .save_stream_with_etag(
                key,
                "REMOTE-ETAG",
                create_mock_stream(b"new streamed data".to_vec(), 4),
            )
            .await
            .unwrap();

        assert_eq!(
            cache.get(key).await.unwrap().as_deref(),
            Some("REMOTE-ETAG")
        );
        assert_eq!(cache.get_data(key).await.unwrap(), b"new streamed data");
    }

    #[tokio::test]
    async fn test_save_stream_with_etag_preserves_cache_on_stream_error() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let cache = LocalObjectStore::new(temp_dir.path().to_path_buf());
        let key = "stream-error";
        cache.save_bytes(key, b"old data").await.unwrap();
        let old_etag = cache.get(key).await.unwrap();
        let stream = futures_util::stream::iter(vec![
            Ok(Bytes::from_static(b"partial")),
            Err(io::Error::other("simulated error")),
        ]);

        let result = cache
            .save_stream_with_etag(key, "NEW-ETAG", Box::pin(stream))
            .await;

        assert!(matches!(
            result,
            Err(crate::caches::CacheError::StreamError)
        ));
        assert_eq!(cache.get(key).await.unwrap(), old_etag);
        assert_eq!(cache.get_data(key).await.unwrap(), b"old data");
        let files = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(files.iter().all(|name| !name.contains(".tmp.")));
    }

    #[tokio::test]
    async fn test_save_stream_with_etag_rejects_empty_etag() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let cache = LocalObjectStore::new(temp_dir.path().to_path_buf());

        let result = cache
            .save_stream_with_etag("file", " ", create_mock_stream(vec![], 1))
            .await;

        assert!(matches!(
            result,
            Err(crate::caches::CacheError::InvalidEtag)
        ));
        assert!(cache.get("file").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_chunked_save_rejects_empty_etag_and_removes_temp_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let cache = LocalObjectStore::new(temp_dir.path().to_path_buf());
        let handle = cache.begin_chunked_save("file").await.unwrap();
        handle.write_chunk(b"partial").await.unwrap();

        let result = handle.finalize(" ").await;

        assert!(matches!(
            result,
            Err(crate::caches::CacheError::InvalidEtag)
        ));
        assert!(cache.get("file").await.unwrap().is_none());
        let files = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_set_etag_without_rewriting_data() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let cache = LocalObjectStore::new(temp_dir.path().to_path_buf());
        cache.save_bytes("file", b"cached data").await.unwrap();

        cache.set_etag("file", "REMOTE-ETAG").await.unwrap();

        assert_eq!(
            cache.get("file").await.unwrap().as_deref(),
            Some("REMOTE-ETAG")
        );
        assert_eq!(cache.get_data("file").await.unwrap(), b"cached data");
    }

    #[tokio::test]
    async fn test_get_all_entries_includes_file_size() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let cache = LocalObjectStore::new(temp_dir.path().to_path_buf());
        cache.save_bytes("diary/file", b"12345").await.unwrap();

        let entries = cache.get_all_entries().await.unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "diary/file");
        assert_eq!(entries[0].size, 5);
    }

    #[tokio::test]
    async fn test_get_all_entries_treats_missing_root_as_empty() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path = temp_dir.path().join("not-created");
        let cache = LocalObjectStore::new(missing_path.clone());

        assert!(!missing_path.exists());
        assert!(cache.get_all_entries().await.unwrap().is_empty());
        assert!(cache.get_all().await.unwrap().is_empty());
        assert!(!missing_path.exists());
    }

    #[tokio::test]
    async fn test_get_all() {
        use std::collections::HashMap;

        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let cache = LocalObjectStore::new(path);
        let entries: Vec<(&str, &[u8])> = vec![
            ("a", b"data_a"),
            ("b/c", b"data_bc"),
            ("d/e/f", b"data_def"),
            ("g", b"data_g"),
        ];

        let mut expected = HashMap::new();
        for (key, data) in &entries {
            cache.save_bytes(key, data).await.unwrap();
            expected.insert(key.to_string(), md5_hex(data));
        }

        let all = cache.get_all().await.unwrap();
        assert_eq!(all.len(), entries.len());

        for (key, md5) in all {
            assert_eq!(expected.get(&key), Some(&md5), "Key: {}", key);
        }

        // 测试空存储
        for (key, _) in &entries {
            cache.delete(key).await.unwrap();
        }
        let empty = cache.get_all().await.unwrap();
        assert!(empty.is_empty());
    }
}
