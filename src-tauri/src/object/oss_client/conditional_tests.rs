use super::*;
use crate::stream::collect_data;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn response(status: u16, headers: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} Status\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

async fn server(responses: Vec<Vec<u8>>) -> (OssClient, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client =
        OssClient::with_test_endpoint(&format!("http://{}", listener.local_addr().unwrap()))
            .with_key_prefix("isolated-test".into());
    let handle = tokio::spawn(async move {
        let mut requests = vec![];
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            let mut buffer = [0; 1024];
            while !bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                let n = socket.read(&mut buffer).await.unwrap();
                assert_ne!(n, 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            requests.push(String::from_utf8(bytes).unwrap());
            socket.write_all(&response).await.unwrap();
        }
        requests
    });
    (client, handle)
}

#[tokio::test]
async fn conditional_get_is_one_request_and_does_not_leak_headers() {
    let (client, handle) = server(vec![response(200, "", "abc"), response(200, "", "xyz")]).await;
    assert_eq!(
        collect_data(
            client
                .download_stream_if_match("a", "old-etag")
                .await
                .unwrap()
        )
        .await
        .unwrap(),
        b"abc"
    );
    assert_eq!(
        collect_data(client.download_stream("b").await.unwrap())
            .await
            .unwrap(),
        b"xyz"
    );
    let requests = handle.await.unwrap();
    assert!(requests[0].starts_with("GET /test-bucket/isolated-test/a HTTP/1.1\r\n"));
    assert!(requests[0].contains("if-match: \"old-etag\"\r\n"));
    assert!(!requests[1].contains("if-match:"));
}

#[tokio::test]
async fn conditional_get_rejects_partial_and_failed_responses_and_truncation() {
    for status in [204, 206, 304, 403, 404, 412, 500] {
        let (client, handle) = server(vec![response(status, "", "error")]).await;
        let error = match client.download_stream_if_match("a", "old").await {
            Err(error) => error,
            Ok(_) => panic!("unexpected success for {status}"),
        };
        assert!(error.to_string().contains(&format!("HTTP {status}")));
        assert_eq!(handle.await.unwrap().len(), 1);
    }
    let (client, handle) = server(vec![
        b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc".to_vec(),
    ])
    .await;
    assert!(
        collect_data(client.download_stream_if_match("a", "old").await.unwrap())
            .await
            .is_err()
    );
    assert_eq!(handle.await.unwrap().len(), 1);
}

#[tokio::test]
async fn unsafe_etag_conditions_are_rejected_before_network_access() {
    for etag in ["", "*", "\"quoted\"", "W/weak", "a\r\nX-Injected: yes"] {
        assert!(etag_header(etag).is_err());
    }
}

#[tokio::test]
async fn conditional_copy_signs_oss_headers_encodes_source_and_keeps_real_result() {
    let xml = "<CopyObjectResult><ETag>\"destination-etag\"</ETag><LastModified>now</LastModified></CopyObjectResult>";
    let (client, handle) = server(vec![
        response(
            200,
            "ETag: \"destination-etag\"\r\nx-oss-hash-crc64ecma: 18446744073709551615\r\n",
            xml,
        ),
        response(200, "", ""),
    ])
    .await;
    let result = client
        .copy_object_if_match("/目录/a b?%#.enc", "backup", "source-etag", true)
        .await
        .unwrap();
    assert_eq!(result.etag, "destination-etag");
    assert_eq!(result.crc64, Some(u64::MAX));
    collect_data(client.download_stream("a").await.unwrap())
        .await
        .unwrap();
    let requests = handle.await.unwrap();
    let request = &requests[0];
    assert!(request.starts_with("PUT /test-bucket/isolated-test/backup HTTP/1.1\r\n"));
    assert!(request.contains(
        "x-oss-copy-source: /test-bucket/isolated-test/%E7%9B%AE%E5%BD%95/a%20b%3F%25%23.enc\r\n"
    ));
    assert!(request.contains("x-oss-copy-source-if-match: \"source-etag\"\r\n"));
    assert!(request.contains("x-oss-forbid-overwrite: true\r\n"));
    assert!(request
        .lines()
        .find(|v| v.starts_with("authorization:"))
        .unwrap()
        .contains("OSS4-HMAC-SHA256"));
    assert!(!request.contains("x-amz-copy-source:"));
    assert!(!requests[1].contains("x-oss-copy-source"));
    assert!(!requests[1].contains("x-oss-forbid-overwrite"));
}

#[test]
fn oss_v4_signature_matches_the_documented_example_and_independent_hmac() {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("content-disposition", "attachment"),
        ("content-length", "3"),
        ("content-md5", "ICy5YqxZB1uWSwcVLSNLcA=="),
        ("content-type", "text/plain"),
        ("x-oss-content-sha256", "UNSIGNED-PAYLOAD"),
        ("x-oss-date", "20250411T064124Z"),
    ] {
        headers.insert(
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_static(value),
        );
    }
    let auth = oss_authorization(
        "example-id",
        "yourAccessKeySecret",
        "cn-hangzhou",
        "/examplebucket/exampleobject",
        &headers,
        &["content-disposition", "content-length"],
    )
    .unwrap();
    // 规范化请求 SHA 为官方示例的 c46d9639…36eca。官方展示的最终签名使用
    // 隐去的凭据，不可直接复用；这里以文档的占位 SK 用 .NET HMAC 独立核算。
    assert!(auth
        .to_str()
        .unwrap()
        .ends_with("Signature=d3694c2dfc5371ee6acd35e88c4871ac95a7ba01d3a2f476768fe61218590097"));
    assert!(auth.is_sensitive());
}

#[tokio::test]
async fn conditional_copy_accepts_absent_crc_but_rejects_malformed_or_error_results() {
    let valid = "<CopyObjectResult><ETag>\"new\"</ETag></CopyObjectResult>";
    let (client, handle) = server(vec![response(200, "", valid)]).await;
    assert!(client
        .copy_object_if_match("a", "b", "old", false)
        .await
        .unwrap()
        .crc64
        .is_none());
    assert!(handle.await.unwrap()[0].contains("x-oss-forbid-overwrite: false\r\n"));
    for (status, headers, body) in [
        (409, "", "exists"),
        (412, "", "changed"),
        (500, "", "error"),
        (200, "", "<Error><Code>InternalError</Code></Error>"),
        (200, "", "<Unexpected><ETag>new</ETag></Unexpected>"),
        (
            200,
            "",
            "<CopyObjectResult><ETag></ETag></CopyObjectResult>",
        ),
        (200, "", "<CopyObjectResult><ETag>new</ETag>"),
        (200, "ETag: \"different\"\r\n", valid),
        (200, "x-oss-hash-crc64ecma: -1\r\n", valid),
        (200, "x-oss-hash-crc64ecma: 18446744073709551616\r\n", valid),
    ] {
        let (client, handle) = server(vec![response(status, headers, body)]).await;
        assert!(
            client
                .copy_object_if_match("a", "b", "old", true)
                .await
                .is_err(),
            "{status} {headers} {body}"
        );
        assert_eq!(handle.await.unwrap().len(), 1);
    }
    let (client, handle) = server(vec![response(200, "", &"x".repeat(65537))]).await;
    assert!(client
        .copy_object_if_match("a", "b", "old", true)
        .await
        .is_err());
    handle.await.unwrap();
}

#[tokio::test]
async fn metadata_preserves_optional_crc64_and_rejects_invalid_numbers() {
    for (crc, expected) in [
        ("", Some(None)),
        ("123", Some(Some(123))),
        ("-1", None),
        ("not-a-number", None),
    ] {
        let headers = if crc.is_empty() {
            String::new()
        } else {
            format!("x-oss-hash-crc64ecma: {crc}\r\n")
        };
        let (client, handle) = server(vec![response(200, &headers, "")]).await;
        let result = client.get_metadata_if_exists("a").await;
        match expected {
            Some(value) => assert_eq!(result.unwrap().unwrap().crc64, value),
            None => assert!(result.is_err()),
        }
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn real_oss_conditions_prevent_overwrite_and_return_copy_crc_when_available() {
    let client = OssClient::from_env();
    assert_eq!(
        std::env::var("ALIYUN_BUCKET_NAME").unwrap(),
        "surkaa-test",
        "此测试只允许测试桶"
    );
    let (client, guard) = crate::test_utils::TestOssGuard::new(client).await;
    let source = "目录/a b?%#.enc";
    let etag = client.upload_bytes(source, b"123456789").await.unwrap();
    assert!(
        matches!(client.download_stream_if_match(source, "wrong-etag").await, Err(error) if error.to_string().contains("412"))
    );
    assert_eq!(
        collect_data(
            client
                .download_stream_if_match(source, &etag)
                .await
                .unwrap()
        )
        .await
        .unwrap(),
        b"123456789"
    );
    client
        .upload_bytes("existing", b"do not overwrite")
        .await
        .unwrap();
    for (target, condition, forbid, expected_status) in [
        ("absent", "wrong-etag", true, "412"),
        ("existing", "wrong-etag", false, "412"),
        ("existing", etag.as_str(), true, "409"),
    ] {
        let error = match client
            .copy_object_if_match(source, target, condition, forbid)
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("condition must be enforced"),
        };
        assert!(error.to_string().contains(expected_status), "{error}");
    }
    assert!(!client.object_exists("absent").await.unwrap());
    assert_eq!(
        client.download_bytes("existing").await.unwrap(),
        b"do not overwrite"
    );
    let copied = client
        .copy_object_if_match(source, "backup", &etag, true)
        .await
        .unwrap();
    let head = client.get_metadata("backup").await.unwrap();
    assert_eq!(head.content_length, Some(9));
    assert_eq!(head.etag.as_deref(), Some(copied.etag.as_str()));
    if let Some(crc) = head.crc64.or(copied.crc64) {
        assert_eq!(crc, 0x995dc9bbdf1939fa);
    }
    println!(
        "[OSS TEST] copy CRC available: {}",
        head.crc64.or(copied.crc64).is_some()
    );
    assert_eq!(client.download_bytes("backup").await.unwrap(), b"123456789");
    guard.cleanup().await;
}
