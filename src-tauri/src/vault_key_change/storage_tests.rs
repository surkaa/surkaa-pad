use super::{RekeyStorage, Storage};
use crate::caches::LocalObjectStore;
use crate::cryptos::Crypto;
use crate::object::OssClient;
use crate::state::AppState;
use crate::vault_key_change::progress::FileProgress;
use crate::vault_key_change::types::{KeyChangeFileStep, KeyChangeProgress};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn cloud_read_uses_one_get_without_an_unused_size_query() {
    let (client, server) = OssClient::with_test_response(
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc".to_vec(),
    )
    .await;
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage {
        remote: true,
        state: AppState::from_parts(
            Crypto::new(),
            client,
            LocalObjectStore::new(temp.path().into()),
        ),
    };
    let stream = storage.read("object").await.unwrap();
    assert_eq!(crate::stream::collect_data(stream).await.unwrap(), b"abc");
    assert_eq!(
        server.await.unwrap(),
        "GET /test-bucket/isolated-test/object HTTP/1.1"
    );
}

fn tracker(
    size: u64,
    step: KeyChangeFileStep,
    events: Arc<Mutex<Vec<KeyChangeProgress>>>,
) -> FileProgress {
    FileProgress::new(
        KeyChangeProgress {
            phase: "committing".into(),
            completed_files: 0,
            total_files: 1,
            completed_bytes: 0,
            total_bytes: size,
            current_file: Some("target".into()),
            file_progress: None,
        },
        step,
        size,
        Arc::new(move |p| events.lock().unwrap().push(p)),
    )
}

#[tokio::test]
async fn cloud_copy_never_downloads_or_reuploads_even_above_five_gib() {
    for (size, reported_size, copy_status) in [
        (1024 * 1024 * 1024 - 1, 1024 * 1024 * 1024 - 1, 200),
        (1024 * 1024 * 1024, 1024 * 1024 * 1024, 200),
        (1024 * 1024 * 1024 + 1, 1024 * 1024 * 1024 + 1, 200),
        (6 * 1024 * 1024 * 1024, 6 * 1024 * 1024 * 1024, 200),
        (6 * 1024 * 1024 * 1024, 6 * 1024 * 1024 * 1024 - 1, 200),
        (6 * 1024 * 1024 * 1024, 6 * 1024 * 1024 * 1024, 403),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client =
            OssClient::with_test_endpoint(&format!("http://{}", listener.local_addr().unwrap()))
                .with_key_prefix("isolated-test".into());
        let server = tokio::spawn(async move {
            let mut methods = vec![];
            for _ in 0..if copy_status == 200 { 3 } else { 1 } {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![];
                let mut buffer = [0_u8; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let read = socket.read(&mut buffer).await.unwrap();
                    assert_ne!(read, 0);
                    request.extend_from_slice(&buffer[..read]);
                }
                let request = String::from_utf8(request).unwrap();
                let method = request.split_whitespace().next().unwrap().to_owned();
                assert!(
                    request.starts_with(&format!("{method} /test-bucket/isolated-test/target "))
                );
                let body = if method == "PUT" {
                    // rust-s3 的 CopyObject 使用兼容的 x-amz-copy-source。
                    assert!(request.to_ascii_lowercase().contains("x-amz-copy-source:"));
                    assert!(request.contains("test-bucket/isolated-test/source"));
                    "<CopyObjectResult><ETag>\"copy-etag\"</ETag></CopyObjectResult>"
                } else {
                    assert_eq!(method, "HEAD", "不允许 GET 源文件或 multipart 重传");
                    ""
                };
                let length = if method == "HEAD" {
                    reported_size
                } else {
                    body.len() as u64
                };
                let status = if method == "PUT" { copy_status } else { 200 };
                let response = format!("HTTP/1.1 {status} Status\r\nContent-Length: {length}\r\nETag: \"copy-etag\"\r\nConnection: close\r\n\r\n{body}");
                socket.write_all(response.as_bytes()).await.unwrap();
                methods.push(method);
            }
            methods
        });
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage {
            remote: true,
            state: AppState::from_parts(
                Crypto::new(),
                client,
                LocalObjectStore::new(temp.path().into()),
            ),
        };
        let events = Arc::new(Mutex::new(vec![]));
        let copying = tracker(size, KeyChangeFileStep::BackingUp, events.clone());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            storage.copy("source", "target", size, &copying),
        )
        .await
        .unwrap();
        if copy_status != 200 {
            assert!(result.unwrap_err().message.contains("403"));
            assert_eq!(
                events.lock().unwrap().len(),
                1,
                "复制失败不得静默转为下载重传或报告完成"
            );
        } else if size == reported_size {
            assert_eq!(result.unwrap(), "copy-etag");
            copying.finish();
            let captured = events.lock().unwrap();
            let points: Vec<_> = captured
                .iter()
                .map(|e| e.file_progress.as_ref().unwrap().completed_points)
                .collect();
            assert_eq!(
                points,
                if size < 1024 * 1024 * 1024 {
                    vec![0, 10]
                } else {
                    vec![0, 100]
                }
            );
        } else {
            assert!(result.unwrap_err().message.contains("大小不匹配"));
            assert_eq!(events.lock().unwrap().len(), 1, "失败不报告完成");
        }
        assert_eq!(
            server.await.unwrap(),
            if copy_status == 200 {
                vec!["PUT", "HEAD", "HEAD"]
            } else {
                vec!["PUT"]
            }
        );
    }
}

#[tokio::test]
#[ignore = "显式设置 surkaa-test 后运行：在云端构造 > 1 GiB 对象并验证大对象复制"]
async fn real_cloud_copy_above_one_gib_cleans_all_test_objects() {
    assert_eq!(
        std::env::var("ALIYUN_BUCKET_NAME").unwrap(),
        "surkaa-test",
        "大对象测试仅允许测试桶"
    );
    let (client, guard) = crate::test_utils::TestOssGuard::new(OssClient::from_env()).await;
    client.require_unversioned_bucket().await.unwrap();
    let seed = "large/seed";
    let seed_size = 8 * 1024 * 1024;
    let data: Vec<u8> = (0..seed_size).map(|i| (i % 251) as u8).collect();
    client.upload_bytes(seed, &data).await.unwrap();
    let mut current = seed.to_owned();
    let mut size = seed_size as u64;
    // 七次倍增后为 1 GiB，再追加一个 8 MiB 分片；本机只上传 seed。
    for index in 0..7 {
        let next = format!("large/double-{index}");
        client
            .concatenate_test_objects(&next, &[&current, &current])
            .await
            .unwrap();
        size *= 2;
        assert_eq!(
            client.get_metadata(&next).await.unwrap().content_length,
            Some(size)
        );
        if current != seed {
            client.delete(&current).await.unwrap();
        }
        current = next;
    }
    let source = "large/source";
    client
        .concatenate_test_objects(source, &[&current, seed])
        .await
        .unwrap();
    size += seed_size as u64;
    client.delete(&current).await.unwrap();
    assert!(size > 1024 * 1024 * 1024);
    assert_eq!(
        client.get_metadata(source).await.unwrap().content_length,
        Some(size)
    );
    println!("[OSS TEST] cloud copy object size={size}");
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage {
        remote: true,
        state: AppState::from_parts(
            Crypto::new(),
            client.clone(),
            LocalObjectStore::new(temp.path().into()),
        ),
    };
    client
        .upload_bytes("large/target", b"old target before replacement")
        .await
        .unwrap();
    for (source, target, action) in [
        (source, "large/backup", KeyChangeFileStep::BackingUp),
        (
            "large/backup",
            "large/target",
            KeyChangeFileStep::WritingTarget,
        ),
    ] {
        let copying = tracker(size, action, Arc::new(Mutex::new(vec![])));
        let etag = storage.copy(source, target, size, &copying).await.unwrap();
        copying.finish();
        assert!(!etag.is_empty());
        assert_eq!(
            client.get_metadata(target).await.unwrap().content_length,
            Some(size)
        );
        for offset in [0, size / 2, 1024 * 1024 * 1024 - 32, size - 64] {
            let (stream, length) = client
                .download(target, Some((offset, offset + 63)))
                .await
                .unwrap();
            assert_eq!(length, 64);
            let bytes = crate::stream::collect_data(stream).await.unwrap();
            let expected: Vec<_> = (offset..offset + 64)
                .map(|i| ((i % seed_size as u64) % 251) as u8)
                .collect();
            assert_eq!(bytes, expected);
        }
    }
    assert!(client.list_multipart_uploads("").await.unwrap().is_empty());
    guard.cleanup().await;
    assert!(
        client.list_all_keys("").await.unwrap().is_empty(),
        "大对象测试必须如期清理"
    );
}
