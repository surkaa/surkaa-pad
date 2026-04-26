use crate::attachments::attachment::get_attachment_stream;
use crate::attachments::{AttachmentError, AttachmentMeta};
use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
use crate::cryptos::CryptoError;
use crate::diaries::{get_diary, DiaryError};
use crate::object::{ObjectError, OssClient};
use crate::state::AppState;
use crate::storages::remote_attachments_key;
use crate::stream::collect_data_with_capacity;
use chrono::Utc;
use http_range_header::parse_range_header;
use std::cmp::min;
use tauri::http::header::{
    ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE,
};
use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder, Wry};
use tauri_plugin_log::log;

pub const PROTOCOL_NAME: &str = "attachment";
const MAX_CHUNK_SIZE: u64 = 1024 * 1024; // 1MB

/// 统一协议错误路由
#[derive(Debug)]
enum ProtocolError {
    BadRequest(&'static str),
    Forbidden(&'static str),
    NotFound(&'static str),
    /// 处理 416 错误
    RangeNotSatisfiable(u64),
    Internal(String),
}

impl ProtocolError {
    fn into_response(self) -> Response<Vec<u8>> {

        let (status, msg) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m.to_string()),
            Self::Forbidden(m) => (StatusCode::FORBIDDEN, m.to_string()),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, format!("{} not found", m)),
            Self::RangeNotSatisfiable(size) => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(CONTENT_RANGE, format!("bytes */{}", size))
                    .body(vec![])
                    .unwrap_or_default();
            }
            Self::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        Response::builder()
            .status(status)
            .body(msg.into_bytes())
            .unwrap_or_default()
    }
}

impl From<DiaryError> for ProtocolError {
    fn from(e: DiaryError) -> Self { ProtocolError::Internal(e.to_string()) }
}
impl From<CryptoError> for ProtocolError {
    fn from(e: CryptoError) -> Self { ProtocolError::Internal(e.to_string()) }
}
impl From<AttachmentError> for ProtocolError {
    fn from(e: AttachmentError) -> Self { ProtocolError::Internal(e.to_string()) }
}
impl From<ObjectError> for ProtocolError {
    fn from(e: ObjectError) -> Self { ProtocolError::Internal(e.to_string()) }
}
impl From<std::io::Error> for ProtocolError {
    fn from(e: std::io::Error) -> Self { ProtocolError::Internal(e.to_string()) }
}

pub fn attachment_protocol(
    context: UriSchemeContext<Wry>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app_handle = context.app_handle();
    let state = app_handle.state::<AppState>().inner().clone();
    log::info!("收到附件协议请求: {}", request.uri());
    tauri::async_runtime::spawn(async move {
        let response = process_attachment(state, request)
            .await
            .unwrap_or_else(|e| {
                tauri_plugin_log::log::error!("Protocol error: {:?}", e);
                e.into_response()
            });
        responder.respond(response);
    });
}

/// 格式：/id/filename
async fn process_attachment(
    state: AppState,
    request: Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, ProtocolError> {
    let client = state
        .get_client()
        .map_err(|_| ProtocolError::Internal("OSS client not ready".into()))?;
    let cache = state.diary_cache();
    let lfc = state.local_file_cache();
    let crypto = state.crypto();

    // Slice Pattern Matching 路由硬解
    let path = request.uri().path().trim_start_matches('/');
    let segments: Vec<&str> = path.split('/').collect();
    let [id, encoded_filename] = segments.as_slice() else {
        return Err(ProtocolError::BadRequest("Invalid URI path structure"));
    };

    // 对 filename 进行 URL 解码，处理中文或特殊字符
    let filename = urlencoding::decode(encoded_filename)
        .map_err(|_| ProtocolError::BadRequest("Invalid URL encoding in filename"))?
        .into_owned();

    let diary = get_diary(&cache, &lfc, &crypto, &client, id).await?;

    let attachment = diary
        .attachments
        .iter()
        // 使用解码后的原始文件名进行匹配
        .find(|a| a.filename == filename)
        .ok_or(ProtocolError::NotFound("attachment"))?;

    if attachment.algorithm == Gcm {
        return Err(ProtocolError::Forbidden("GCM decryption is not supported"));
    }

    // 使用 http-range-header 解析 Range 请求头
    let file_size = attachment.size;
    let range_header_val = request.headers().get(RANGE).and_then(|v| v.to_str().ok());

    let (start, end, is_range) = match range_header_val {
        Some(raw_range) => {
            let ranges = parse_range_header(raw_range).map_err(|e| {
                log::error!("Parse range header error: {:?}", e);
                ProtocolError::BadRequest("Invalid Range format")
            })?;
            let valid_ranges = ranges.validate(file_size).map_err(|e| {
                log::error!("Parse range header error: {:?}", e);
                ProtocolError::RangeNotSatisfiable(file_size)
            })?;
            let r = &valid_ranges[0];
            let s = *r.start();
            // 应用 MAX_CHUNK_SIZE 限制，防止内存溢出
            let e = min(*r.end(), s + MAX_CHUNK_SIZE - 1);
            (s, e, true)
        }
        None => (0, file_size.saturating_sub(1), false),
    };

    let key = remote_attachments_key(id, &filename);
    let (stream, len) = get_attachment_stream(&key, &lfc, &client, Some((start, end))).await?;

    let stream = if attachment.encrypted {
        crypto.decrypt_streaming(stream, &attachment.nonce, start)?
    } else {
        stream
    };

    // 消费 Stream，将其收集到内存中的 Vec<u8>
    // 因为这只是整个文件中的一个 Range Chunk（切片），所以放进内存是安全的
    let data = collect_data_with_capacity(stream, len as usize).await?;

    let status = if is_range {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };

    let mut builder = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, &attachment.mimetype)
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(ACCEPT_RANGES, "bytes")
        .header(CONTENT_LENGTH, data.len());

    if is_range {
        builder = builder.header(
            CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, file_size),
        );
    }

    builder
        .body(data)
        .map_err(|e| ProtocolError::Internal(e.to_string()))
}

pub fn get_full_attachment_url(
    id: &str,
    attachment: &AttachmentMeta,
    client: &OssClient,
) -> Result<String, ObjectError> {
    // 对文件名进行 URL Encode
    let encoded_filename = urlencoding::encode(&attachment.filename);

    if attachment.encrypted {
        let timestamp = Utc::now().timestamp();
        Ok(format!(
            "http://{}.localhost/{}/{}?t={}",
            PROTOCOL_NAME, id, encoded_filename, timestamp
        ))
    } else {
        // TODO 考虑针对未加密的附件也尝试访问缓存
        let key = remote_attachments_key(id, &attachment.filename);
        let url = client.direct_url(&key)?;
        Ok(url)
    }
}
