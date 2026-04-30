#[cfg(test)]
mod tests {
    use crate::object::object_types::STREAM_MIME_TYPE;
    use crate::object::OssClient;
    use crate::stream::{collect_data, create_mock_stream, ByteStream};
    use bytes::Bytes;

    use futures_util::stream::iter;
    use futures_util::TryStreamExt;
    use serial_test::serial;
    use std::io::Error;
    use std::iter::once;

    async fn assert_empty(client: &OssClient, msg: &str) {
        // 检查有没有遗留的测试文件
        let (objects, next_token) = client.list("", None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "{}", msg);
        if objects.len() != 0 {
            panic!("{}: 发现遗留对象 {:?}", msg, objects);
        }
    }

    async fn add_object(client: &OssClient, key: &str, content: &'static str) {
        let len = content.len() as u64;
        let bytes = Bytes::from_static(content.as_bytes());
        let stream: ByteStream = Box::pin(iter(once(Ok::<_, Error>(bytes))));
        client
            .upload(key, len, stream, STREAM_MIME_TYPE)
            .await
            .expect("上传失败");
    }

    #[serial]
    #[tokio::test]
    async fn test_oss() {
        let client = OssClient::from_env();
        let key = "test_upload.txt";
        let content = "This is a test line for OSS upload and download testing.";
        let repeat_count = 1000;

        assert_empty(&client, "测试开始前对象存储应为空").await;

        // 生成测试文件
        let dir = tempfile::tempdir().expect("无法创建临时目录");
        let file_path = dir.path().join(key);
        // 写入大量数据以测试上传
        let file_size = {
            let mut file = std::fs::File::create(&file_path).expect("无法创建测试文件");
            for _ in 0..repeat_count {
                use std::io::Write;
                writeln!(file, "{}", content).expect("无法写入测试文件");
            }
            // {}会自动关闭文件
            file.metadata().expect("无法获取文件元数据").len()
        };
        dbg!(&file_size);
        // 计算md5
        let file_content = std::fs::read(&file_path).expect("无法读取测试文件");
        let md5_etag = format!("{:X}", md5::compute(&file_content));

        // 上传文件
        let file = tokio::fs::File::open(file_path)
            .await
            .expect("无法打开测试文件");
        let mut uploaded: u64 = 0;
        let stream = tokio_util::io::ReaderStream::new(file).map_ok(move |chunk| {
            uploaded += chunk.len() as u64;
            let percentage = (uploaded as f64 / file_size as f64) * 100.0;
            println!(
                "🚀 上传进度: {:.2}% ({}/{})",
                percentage, uploaded, file_size
            );
            chunk
        });
        let stream: ByteStream = Box::pin(stream);
        client
            .upload(key, file_size, stream, STREAM_MIME_TYPE)
            .await
            .expect("上传失败");

        // 获取元数据
        let metadata = client.get_metadata(key).await.expect("获取元数据失败");
        assert_eq!(metadata.content_length, Some(file_size));
        assert_eq!(metadata.etag.as_deref(), Some(md5_etag.as_str()));

        // 列出对象
        let (objects, next_token) = client.list("", None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "不应有续页");
        assert_eq!(objects.len(), 1, "应列出一个对象");
        let obj = &objects[0];
        assert_eq!(obj.key, key);
        assert_eq!(obj.size, file_size);
        assert_eq!(obj.etag.as_deref(), Some(md5_etag.as_str()));

        // 下载对象
        let (mut download_stream, _) = client.download(key, None).await.expect("下载失败");
        let mut downloaded_data = Vec::new();
        while let Some(chunk) = download_stream.try_next().await.expect("读取下载流失败") {
            downloaded_data.extend_from_slice(&chunk);
        }
        assert_eq!(
            downloaded_data, file_content,
            "下载的数据应与上传的数据匹配"
        );

        // 删除对象
        client.delete(key).await.expect("删除失败");

        // 确认删除
        assert_empty(&client, "测试结束后对象存储应为空").await;
    }

    #[serial]
    #[tokio::test]
    async fn test_batch_delete() {
        let client = OssClient::from_env();
        assert_empty(&client, "测试开始前对象存储应为空").await;

        // 上传多个测试文件
        let prefix = "id_";
        let keys: Vec<String> = (0..5).map(|i| format!("{}{}", prefix, i)).collect();
        for key in &keys {
            let content = "This is a test file for batch delete.";
            let len = content.len() as u64;
            let bytes = Bytes::from_static(content.as_bytes());
            let stream: ByteStream = Box::pin(iter(once(Ok::<_, Error>(bytes))));
            client
                .upload(key, len, stream, STREAM_MIME_TYPE)
                .await
                .expect("上传失败");
        }

        // 确认上传
        let (objects, next_token) = client.list(prefix, None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "不应有续页");
        assert_eq!(objects.len(), keys.len(), "应列出所有上传的对象");
        dbg!(&objects);

        // 批量删除 使用通配符会删除失败
        client.delete("id_*").await.expect("批量删除失败");
        // 确认删除失败
        let (objects, next_token) = client.list(prefix, None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "不应有续页");
        assert_eq!(objects.len(), keys.len(), "对象不应被删除");

        // 使用前缀删除
        let delete_keys = client
            .delete_with_prefix(prefix)
            .await
            .expect("前缀删除失败");
        assert_eq!(delete_keys.len(), keys.len(), "应删除所有上传的对象");
        // 确认删除
        assert_empty(&client, "测试结束后对象存储应为空").await;
    }

    #[serial]
    #[tokio::test]
    async fn test_list() {
        let client = OssClient::from_env();
        assert_empty(&client, "测试开始前对象存储应为空").await;
        add_object(&client, "folder/test1.txt", "Test file 1").await;
        add_object(&client, "folder/test2.txt", "Test file 2").await;
        add_object(&client, "folder/subfolder/test3.txt", "Test file 3").await;

        // 列出对象
        let (objects, next_token) = client.list("", None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "不应有续页");
        assert_eq!(objects.len(), 3, "应列出三个对象");
        let keys: Vec<String> = objects.iter().map(|obj| obj.key.clone()).collect();
        assert!(keys.contains(&"folder/test1.txt".to_string()));
        assert!(keys.contains(&"folder/test2.txt".to_string()));
        assert!(keys.contains(&"folder/subfolder/test3.txt".to_string()));

        // 清理
        client
            .delete_with_prefix("folder/")
            .await
            .expect("删除失败");
        assert_empty(&client, "测试结束后对象存储应为空").await;
    }

    #[serial]
    #[tokio::test]
    async fn test_download_range() {
        let client = OssClient::from_env();
        assert_empty(&client, "测试开始前对象存储应为空").await;
        let key = "test_range.txt";
        let content = "This is a test file for range download.";
        add_object(&client, key, content).await;

        // 下载部分内容
        let (mut download_stream, len) =
            client.download(key, Some((5, 15))).await.expect("下载失败");
        let mut downloaded_data = Vec::new();
        while let Some(chunk) = download_stream.try_next().await.expect("读取下载流失败") {
            downloaded_data.extend_from_slice(&chunk);
        }
        let downloaded_str = String::from_utf8(downloaded_data).expect("下载数据不是有效的UTF-8");
        assert_eq!(len, 11, "下载的内容长度应为请求的范围长度 (15 - 5 + 1)");
        assert_eq!(
            downloaded_str,
            &content[5..=15],
            "下载的范围数据应与原内容匹配"
        );

        // 清理
        client.delete(key).await.expect("删除失败");
        assert_empty(&client, "测试结束后对象存储应为空").await;
    }

    #[serial]
    #[tokio::test]
    async fn test_oss_direct_url() {
        // 1. 初始化客户端 (依赖环境变量)
        let client = OssClient::from_env();
        let test_key = "test_direct_url.txt";
        let test_content = b"Hello OSS Direct URL Test";
        assert_empty(&client, "测试开始前对象存储应为空").await;

        // 2. 先上传一个文件，确保它存在
        client
            .upload_bytes(test_key, &test_content.to_vec())
            .await
            .expect("上传测试文件失败");

        // 3. 生成一个有效期为 60 秒的签名 URL
        let signed_url = client.direct_url(test_key).expect("生成签名URL失败");

        println!("生成的签名URL: {}", signed_url);

        // 4. 使用普通的 reqwest 客户端（不带任何 OSS Header）去请求这个 URL
        let http_client = reqwest::Client::new();
        let resp = http_client
            .get(&signed_url)
            .send()
            .await
            .expect("访问签名URL失败");

        // 5. 验证结果
        let status = resp.status();
        let body = resp.bytes().await.expect("读取响应体失败");

        // 清理测试文件
        let _ = client.delete(test_key).await;

        assert!(
            status.is_success(),
            "签名URL应该可以正常访问，当前状态码: {}",
            status
        );
        assert_eq!(body.as_ref(), test_content, "下载的内容与上传的不一致");
    }

    #[serial]
    #[tokio::test]
    async fn test_upload_etag() {
        let client = OssClient::from_env();
        let test_key = "upload_etag.txt";
        let test_content = b"Hello OSS Uploaded Test";
        assert_empty(&client, "测试开始前对象存储应为空").await;
        let etag = client
            .upload_bytes(test_key, &test_content.to_vec())
            .await
            .expect("上传测试文件失败");
        let md5 = format!("{:X}", md5::compute(&test_content));
        assert_eq!(&etag, &md5, "返回的 ETag 应该是内容的 MD5 值");
        client.delete(test_key).await.expect("删除失败");
        let stream_etag = client
            .upload(
                test_key,
                test_content.len() as u64,
                create_mock_stream(test_content.to_vec(), test_content.len()),
                STREAM_MIME_TYPE,
            )
            .await
            .expect("使用流上传测试文件失败");
        assert_eq!(&stream_etag, &md5, "返回的 ETag 应该是内容的 MD5 值");
        client.delete(test_key).await.expect("删除失败");
        assert_empty(&client, "测试后对象存储应为空").await;
    }

    #[serial]
    #[tokio::test]
    async fn test_rename() {
        let client = OssClient::from_env();
        let test_key = "rename.txt";
        let test_content = b"Hello OSS Renamed Test".to_vec();
        assert_empty(&client, "测试开始前对象存储应为空").await;
        client.upload_bytes(test_key, &test_content.to_vec()).await.expect("上传测试文件失败");
        let new_key = "renamed.txt";
        client.rename(test_key, new_key).await.expect("重命名失败");
        // 确认旧键不存在
        let (objects, next_token) = client.list("", None).await.expect("列出对象失败");
        assert!(next_token.is_none(), "不应有续页");
        assert!(!objects.iter().any(|obj| obj.key == test_key), "旧键仍然存在");
        // 新键存在且内容正确
        let (download_stream, _) = client.download(new_key, None).await.expect("下载失败");
        let downloaded_data = collect_data(download_stream).await.expect("接收下载流失败");
        assert_eq!(test_content, downloaded_data);
        // 上传另一个对象测试不允许有同名对象的重命名
        client.upload_bytes(test_key, &test_content.to_vec()).await.expect("上传测试文件失败");
        let rename_result = client.rename(test_key, new_key).await;
        assert!(rename_result.is_err(), "重命名到已存在的键应该失败");
        // 清理
        client.delete(new_key).await.expect("删除失败");
        client.delete(test_key).await.expect("删除失败");
        assert_empty(&client, "测试结束后对象存储应为空").await;
    }
}
