use super::{ObjectError, OssClient};
use crate::stream::collect_data;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;

impl OssClient {
    pub(crate) async fn with_test_response(response: Vec<u8>) -> (OssClient, JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client =
            OssClient::with_test_endpoint(&format!("http://{}", listener.local_addr().unwrap()))
                .with_key_prefix("isolated-test".into());
        let server = tokio::spawn(async move {
            // 只允许一个请求；额外查询会因监听器关闭而失败。
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![];
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let read = socket.read(&mut buffer).await.unwrap();
                assert_ne!(read, 0);
                request.extend_from_slice(&buffer[..read]);
            }
            socket.write_all(&response).await.unwrap();
            String::from_utf8(request)
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .to_owned()
        });
        (client, server)
    }
}

#[tokio::test]
async fn streaming_get_uses_one_request_and_preserves_empty_and_binary_content() {
    for body in [vec![], vec![0, 1, 128, 255]] {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(&body);
        let (client, server) = OssClient::with_test_response(response).await;
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            client.download_stream("object"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(collect_data(stream).await.unwrap(), body);
        assert_eq!(
            server.await.unwrap(),
            "GET /test-bucket/isolated-test/object HTTP/1.1"
        );
    }
}

#[tokio::test]
async fn streaming_get_accepts_chunked_content_without_a_size_query() {
    let (client, server) = OssClient::with_test_response(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n1\r\na\r\n2\r\nbc\r\n0\r\n\r\n".to_vec()
    ).await;
    assert_eq!(
        collect_data(client.download_stream("object").await.unwrap())
            .await
            .unwrap(),
        b"abc"
    );
    assert_eq!(
        server.await.unwrap(),
        "GET /test-bucket/isolated-test/object HTTP/1.1"
    );
}

#[tokio::test]
async fn streaming_get_rejects_error_status_instead_of_reading_an_error_page_as_ciphertext() {
    for status in [304, 400, 403, 404, 500] {
        let (client, server) = OssClient::with_test_response(
            format!("HTTP/1.1 {status} Status\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .into_bytes(),
        )
        .await;
        let error = match client.download_stream("object").await {
            Ok(_) => panic!("HTTP {status} must not return a data stream"),
            Err(error) => error,
        };
        assert!(error.to_string().contains(&format!("HTTP {status}")));
        assert_eq!(
            server.await.unwrap(),
            "GET /test-bucket/isolated-test/object HTTP/1.1"
        );
    }
}

#[tokio::test]
async fn streaming_get_propagates_truncation_without_a_head_fallback() {
    let (client, server) = OssClient::with_test_response(
        b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc".to_vec(),
    )
    .await;
    let stream = client.download_stream("object").await.unwrap();
    assert!(collect_data(stream).await.is_err());
    assert_eq!(
        server.await.unwrap(),
        "GET /test-bucket/isolated-test/object HTTP/1.1"
    );
}

#[tokio::test]
async fn optional_metadata_uses_one_head_and_preserves_zero_and_large_sizes() {
    for size in [0, 3, 6 * 1024 * 1024 * 1024] {
        let (client, server) = OssClient::with_test_response(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nETag: \"object-etag\"\r\nConnection: close\r\n\r\n"
            ).into_bytes()
        ).await;
        let metadata = client
            .get_metadata_if_exists("/object")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(metadata.content_length, Some(size));
        assert_eq!(metadata.etag.as_deref(), Some("object-etag"));
        assert_eq!(
            server.await.unwrap(),
            "HEAD /test-bucket/isolated-test/object HTTP/1.1"
        );
    }
}

#[tokio::test]
async fn optional_metadata_returns_none_only_for_404() {
    for status in [304, 400, 403, 404, 500] {
        let (client, server) = OssClient::with_test_response(
            format!("HTTP/1.1 {status} Status\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .into_bytes(),
        )
        .await;
        let result = client.get_metadata_if_exists("object").await;
        if status == 404 {
            assert!(result.unwrap().is_none());
        } else {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains(&format!("HTTP {status}")));
        }
        assert_eq!(
            server.await.unwrap(),
            "HEAD /test-bucket/isolated-test/object HTTP/1.1"
        );
    }
}

#[tokio::test]
async fn metadata_and_existence_wrappers_preserve_their_error_semantics_with_one_head() {
    for status in [200, 304, 403, 404, 500] {
        let response = format!(
            "HTTP/1.1 {status} Status\r\nContent-Length: 3\r\nETag: \"object-etag\"\r\nConnection: close\r\n\r\n"
        ).into_bytes();
        let (client, server) = OssClient::with_test_response(response.clone()).await;
        let metadata = client.get_metadata("object").await;
        if status == 200 {
            let metadata = metadata.unwrap();
            assert_eq!(metadata.content_length, Some(3));
            assert_eq!(metadata.etag.as_deref(), Some("object-etag"));
        } else {
            assert!(metadata
                .unwrap_err()
                .to_string()
                .contains(&format!("HTTP {status}")));
        }
        assert_eq!(
            server.await.unwrap(),
            "HEAD /test-bucket/isolated-test/object HTTP/1.1"
        );

        let (client, server) = OssClient::with_test_response(response).await;
        let exists = client.object_exists("object").await;
        match status {
            200 => assert!(exists.unwrap()),
            404 => assert!(!exists.unwrap()),
            _ => assert!(exists
                .unwrap_err()
                .to_string()
                .contains(&format!("HTTP {status}"))),
        }
        assert_eq!(
            server.await.unwrap(),
            "HEAD /test-bucket/isolated-test/object HTTP/1.1"
        );
    }
}

#[tokio::test]
async fn optional_metadata_propagates_initialization_and_transport_errors() {
    assert!(matches!(
        OssClient::new().get_metadata_if_exists("object").await,
        Err(ObjectError::NotInitialized)
    ));
    // 在收到请求后直接断开连接，不能把网络失败当成对象不存在。
    let (client, server) = OssClient::with_test_response(vec![]).await;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.get_metadata_if_exists("object"),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    assert_eq!(
        server.await.unwrap(),
        "HEAD /test-bucket/isolated-test/object HTTP/1.1"
    );
}
