//! 条件只约束本次请求的源对象，不是目标对象的跨设备写锁。
use super::{CopyObjectOutput, ObjectError, OssClient};
use crate::stream::ByteStream;
use futures_util::StreamExt;
use hmac::Mac;
use reqwest::header::{HeaderMap, HeaderValue};
use s3::command::Command;
use s3::request::{tokio_backend::ReqwestRequest, Request};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

fn failed(message: impl ToString) -> ObjectError {
    ObjectError::OperationFailed(message.to_string())
}

pub(super) fn header_text<'a>(
    headers: &'a HeaderMap,
    name: &str,
) -> Result<Option<&'a str>, ObjectError> {
    headers
        .get(name)
        .map(|value| value.to_str().map_err(failed))
        .transpose()
}

pub(super) fn header_u64(headers: &HeaderMap, name: &str) -> Result<Option<u64>, ObjectError> {
    header_text(headers, name)?
        .map(|value| {
            value
                .parse()
                .map_err(|_| failed(format!("无效的 {name} 响应头")))
        })
        .transpose()
}

fn etag_header(etag: &str) -> Result<HeaderValue, ObjectError> {
    // 禁止空值、通配符和弱 ETag，不能把“匹配某一密文”悄悄放宽成“对象存在”。
    if etag.is_empty() || etag == "*" || etag.contains(['"', '\r', '\n']) || etag.starts_with("W/")
    {
        return Err(failed("条件请求需要有效的强 ETag"));
    }
    HeaderValue::from_str(&format!("\"{etag}\"")).map_err(failed)
}

/// rust-s3 的 AWS4 签名不能直接混用 OSS 原生头。这里只为条件 CopyObject 使用
/// OSS4，复用现有凭据、URL 编码和 rustls 连接池，不替换其余 S3 接口。
/// https://help.aliyun.com/zh/oss/developer-reference/recommend-to-use-signature-version-4
fn oss_authorization(
    access_key: &str,
    secret: &str,
    region: &str,
    resource: &str,
    headers: &HeaderMap,
    additional: &[&str],
) -> Result<HeaderValue, ObjectError> {
    let timestamp = header_text(headers, "x-oss-date")?.ok_or_else(|| failed("缺少签名时间"))?;
    let date = timestamp.get(..8).ok_or_else(|| failed("签名时间无效"))?;
    let mut canonical = headers
        .iter()
        .filter(|(name, _)| {
            name.as_str().starts_with("x-oss-")
                || ["content-type", "content-md5"].contains(&name.as_str())
                || additional.contains(&name.as_str())
        })
        .map(|(name, value)| Ok((name.as_str(), value.to_str().map_err(failed)?.trim())))
        .collect::<Result<Vec<_>, ObjectError>>()?;
    // 必须按头名称排序，不可按 name:value 排序；copy-source 是
    // copy-source-if-match 的前缀，冒号会使完整字符串排序结果颠倒。
    canonical.sort_by_key(|(name, _)| *name);
    let canonical: String = canonical
        .into_iter()
        .map(|(name, value)| format!("{name}:{value}\n"))
        .collect();
    let additional = additional.join(";");
    let request = format!(
        "PUT\n{}\n\n{}\n{additional}\nUNSIGNED-PAYLOAD",
        s3::signing::uri_encode(resource, false),
        canonical
    );
    let scope = format!("{date}/{region}/oss/aliyun_v4_request");
    let to_sign = format!(
        "OSS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex::encode(Sha256::digest(request))
    );
    let derive = |key: &[u8], text: &str| -> Result<Zeroizing<[u8; 32]>, ObjectError> {
        let mut hmac = hmac::Hmac::<Sha256>::new_from_slice(key).map_err(failed)?;
        hmac.update(text.as_bytes());
        Ok(Zeroizing::new(hmac.finalize().into_bytes().into()))
    };
    let seed = Zeroizing::new(format!("aliyun_v4{secret}"));
    let date_key = derive(seed.as_bytes(), date)?;
    let region_key = derive(date_key.as_ref(), region)?;
    let service_key = derive(region_key.as_ref(), "oss")?;
    let signing_key = derive(service_key.as_ref(), "aliyun_v4_request")?;
    let signature = derive(signing_key.as_ref(), &to_sign)?;
    let additional = if additional.is_empty() {
        String::new()
    } else {
        format!(",AdditionalHeaders={additional}")
    };
    let mut value = HeaderValue::from_str(&format!(
        "OSS4-HMAC-SHA256 Credential={access_key}/{scope}{additional},Signature={}",
        hex::encode(signature.as_ref())
    ))
    .map_err(failed)?;
    value.set_sensitive(true);
    Ok(value)
}

impl OssClient {
    /// 固定读取指定 ETag 的完整对象，不额外 HEAD，也不降级成无条件 GET。
    pub async fn download_stream_if_match(
        &self,
        key: &str,
        etag: &str,
    ) -> Result<ByteStream, ObjectError> {
        let bucket = self.inner()?;
        let mut headers = bucket.extra_headers.clone();
        headers.insert("if-match", etag_header(etag)?);
        // 只克隆本次请求的头；共享的客户端不能留下 If-Match 影响其他操作。
        let bucket = bucket.with_extra_headers(headers).map_err(failed)?;
        let key = self.physical_key(key);
        let response = bucket.get_object_stream(&key).await.map_err(failed)?;
        if response.status_code != 200 {
            return Err(failed(format!(
                "条件 GET failed: HTTP {}",
                response.status_code
            )));
        }
        Ok(Box::pin(response.bytes.map(|chunk| {
            chunk.map_err(|error| std::io::Error::other(error.to_string()))
        })))
    }

    /// 源 ETag 条件和备份的禁止覆盖都由 OSS 执行。复制响应不明时交由恢复逻辑核查，
    /// 不在这里重试 PUT 或退回下载重传，以免覆盖已存在的恢复资料。
    pub async fn copy_object_if_match(
        &self,
        source: &str,
        target: &str,
        source_etag: &str,
        forbid_overwrite: bool,
    ) -> Result<CopyObjectOutput, ObjectError> {
        let bucket = self.inner()?;
        let mut headers = HeaderMap::new();
        let source = format!("/{}/{}", bucket.name, self.physical_key(source));
        headers.insert(
            "x-oss-copy-source",
            HeaderValue::from_str(&s3::signing::uri_encode(&source, false)).map_err(failed)?,
        );
        headers.insert("x-oss-copy-source-if-match", etag_header(source_etag)?);
        headers.insert(
            "x-oss-forbid-overwrite",
            HeaderValue::from_static(if forbid_overwrite { "true" } else { "false" }),
        );
        let target = self.physical_key(target);
        let request = ReqwestRequest::new(&bucket, &target, Command::GetObject)
            .await
            .map_err(failed)?;
        let url = request.url().map_err(failed)?;
        if url.query().is_some() {
            return Err(failed("条件复制不支持额外查询参数"));
        }
        headers.insert(
            "x-oss-date",
            HeaderValue::from_str(&chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string())
                .map_err(failed)?,
        );
        headers.insert(
            "x-oss-content-sha256",
            HeaderValue::from_static("UNSIGNED-PAYLOAD"),
        );
        let credentials = bucket.credentials().await.map_err(failed)?;
        if let Some(token) = credentials.security_token.or(credentials.session_token) {
            let mut value = HeaderValue::from_str(&token).map_err(failed)?;
            value.set_sensitive(true);
            headers.insert("x-oss-security-token", value);
        }
        let secret = Zeroizing::new(
            credentials
                .secret_key
                .ok_or_else(|| failed("缺少复制凭据"))?,
        );
        let auth = oss_authorization(
            &credentials
                .access_key
                .ok_or_else(|| failed("缺少复制凭据"))?,
            &secret,
            &bucket.region().to_string(),
            &format!("/{}/{target}", bucket.name),
            &headers,
            &[],
        )?;
        headers.insert("authorization", auth);
        // 不自动重试条件 PUT，响应丢失由完整恢复路径处理。
        let response = bucket
            .http_client()
            .put(url)
            .headers(headers)
            .body(Vec::new())
            .send()
            .await
            .map_err(failed)?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let mut stream = response.bytes_stream();
        let mut body = vec![];
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(failed)?;
            if body.len().saturating_add(chunk.len()) > 64 * 1024 {
                return Err(failed("复制响应超过安全上限"));
            }
            body.extend_from_slice(&chunk);
        }
        let body = std::str::from_utf8(&body).map_err(failed)?;
        if status != 200 {
            #[derive(Deserialize)]
            struct ServiceError {
                #[serde(rename = "Code")]
                code: String,
                #[serde(rename = "Message")]
                message: String,
            }
            let detail = quick_xml::de::from_str::<ServiceError>(body)
                .map(|e| {
                    format!("{}: {}", e.code, e.message)
                        .chars()
                        .take(500)
                        .collect::<String>()
                })
                .unwrap_or_default();
            // 不把服务端 StringToSign/签名调试内容写入日志。
            return Err(failed(format!("条件 Copy failed: HTTP {status} {detail}")));
        }
        let mut reader = quick_xml::Reader::from_str(body);
        reader.config_mut().trim_text(true);
        loop {
            match reader.read_event().map_err(failed)? {
                quick_xml::events::Event::Decl(_) | quick_xml::events::Event::Comment(_) => (),
                quick_xml::events::Event::Start(tag)
                    if tag.name().as_ref() == b"CopyObjectResult" =>
                {
                    break
                }
                _ => return Err(failed("复制未返回有效的 CopyObjectResult，保留恢复资料")),
            }
        }
        #[derive(Deserialize)]
        struct CopyResult {
            #[serde(rename = "ETag")]
            etag: String,
        }
        let result: CopyResult = quick_xml::de::from_str(body).map_err(failed)?;
        let etag = result.etag.trim_matches('"').to_owned();
        etag_header(&etag)?;
        if header_text(&headers, "etag")?.is_some_and(|v| v.trim_matches('"') != etag) {
            return Err(failed("复制响应中的 ETag 不一致"));
        }
        Ok(CopyObjectOutput {
            etag,
            crc64: header_u64(&headers, "x-oss-hash-crc64ecma")?,
        })
    }
}

#[cfg(test)]
#[path = "conditional_tests.rs"]
mod tests;
