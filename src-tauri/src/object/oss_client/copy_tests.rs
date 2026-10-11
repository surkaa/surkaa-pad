use super::{ObjectError, OssClient, OssClientInner};
use s3::{creds::Credentials, Bucket, Region};
use std::collections::HashMap;

impl OssClient {
    pub(crate) fn with_test_endpoint(endpoint: &str) -> Self {
        let client = Self::new();
        let credentials =
            Credentials::new(Some("test-key"), Some("test-secret"), None, None, None).unwrap();
        let bucket = Bucket::new(
            "test-bucket",
            Region::Custom {
                region: "cn-hangzhou".into(),
                endpoint: endpoint.into(),
            },
            credentials,
        )
        .unwrap()
        .with_path_style()
        .with_service("oss");
        *client.inner.write().unwrap() = Some(OssClientInner { bucket });
        client
    }

    /// 只用于显式的大对象测试：用云内分片复制拼接小样本，不从本机传输 GiB 数据。
    pub(crate) async fn concatenate_test_objects(
        &self,
        target: &str,
        sources: &[&str],
    ) -> Result<String, ObjectError> {
        let failed = |e: String| ObjectError::OperationFailed(e);
        let upload_id = self
            .initiate_multipart_upload(target, "application/octet-stream")
            .await?;
        let result = async {
            let mut parts = vec![];
            for (index, source) in sources.iter().enumerate() {
                let number = index as u32 + 1;
                let bucket = self
                    .inner()?
                    .with_extra_query(HashMap::from([
                        ("uploadId".into(), upload_id.clone()),
                        ("partNumber".into(), number.to_string()),
                    ]))
                    .map_err(|e| failed(e.to_string()))?;
                let response = bucket
                    .put_object_builder(self.physical_key(target), &[])
                    .with_header(
                        "x-amz-copy-source",
                        format!(
                            "/{}/{}",
                            bucket.name(),
                            urlencoding::encode(&self.physical_key(source))
                        ),
                    )
                    .map_err(|e| failed(e.to_string()))?
                    .execute()
                    .await
                    .map_err(|e| failed(e.to_string()))?;
                if response.status_code() != 200 || response.as_slice().len() > 16 * 1024 {
                    return Err(failed(format!(
                        "测试分片复制失败: HTTP {}",
                        response.status_code()
                    )));
                }
                // PutObject builder 返回的是提取后的 ETag，而非原始 CopyPartResult XML。
                let etag = std::str::from_utf8(response.as_slice())
                    .map_err(|e| failed(e.to_string()))?
                    .trim_matches('"')
                    .to_owned();
                if etag.len() != 32 || !etag.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(failed("测试分片复制的 ETag 无效".into()));
                }
                parts.push((etag, number));
            }
            self.complete_multipart_upload(target, &upload_id, parts)
                .await
        }
        .await;
        if result.is_err() {
            // 测试失败保留对象用于诊断，但必须终止尚未完成的分片会话。
            self.abort_multipart_upload(target, &upload_id).await?;
        }
        result
    }
}
