use super::OssClient;
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
            // 只允许一个请求；旧实现的额外 HEAD 会因监听器关闭而失败。
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
