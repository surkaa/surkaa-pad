use crate::attachments::AttachmentMeta;
use crate::cryptos::types::EncryptionAlgorithm::Gcm;
use crate::cryptos::Crypto;
use crate::diaries::get_diary;
use crate::object::{OssClient, OssState};
use crate::storages::remote_attachments_key;
use chrono::Utc;
use futures_util::StreamExt;
use http_range_header::parse_range_header;
use std::cmp::min;
use tauri::http::header::{
    ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE,
};
use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder, Wry};
use tauri_plugin_log::log;
use crate::caches::DiaryMemoryCache;

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

pub fn attachment_protocol(
    context: UriSchemeContext<Wry>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app_handle = context.app_handle();
    let cache = app_handle.state::<DiaryMemoryCache>().inner().clone();
    let crypto = app_handle.state::<Crypto>().inner().clone();
    let oss_state = app_handle.state::<OssState>().inner().clone();
    log::info!("收到附件协议请求: {}", request.uri());
    tauri::async_runtime::spawn(async move {
        let response = process_attachment(cache, crypto, oss_state, request)
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
    cache: DiaryMemoryCache,
    crypto: Crypto,
    oss_state: OssState,
    request: Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, ProtocolError> {
    let client = oss_state
        .get_client()
        .map_err(|_| ProtocolError::Internal("OSS client not ready".into()))?;

    // Slice Pattern Matching 路由硬解
    let path = request.uri().path().trim_start_matches('/');
    let segments: Vec<&str> = path.split('/').collect();
    let [id, filename] = segments.as_slice() else {
        return Err(ProtocolError::BadRequest("Invalid URI path structure"));
    };

    let diary = get_diary(&cache, &crypto, &client, id)
        .await
        .map_err(|e| ProtocolError::Internal(e.to_string()))?;

    let attachment = diary
        .attachments
        .iter()
        .find(|a| a.filename == *filename)
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

    let key = remote_attachments_key(id, filename);
    let (stream, len) = client
        .download(&key, Some((start, end)))
        .await
        .map_err(|e| ProtocolError::Internal(e.to_string()))?;

    let mut stream = if attachment.encrypted {
        crypto
            .decrypt_streaming(stream, &attachment.nonce, start)
            .map_err(|e| ProtocolError::Internal(e.to_string()))?
    } else {
        stream
    };

    // 消费 Stream，将其收集到内存中的 Vec<u8>
    // 因为这只是整个文件中的一个 Range Chunk（切片），所以放进内存是安全的
    let mut data = Vec::with_capacity(len as usize);
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| ProtocolError::Internal(e.to_string()))?;
        data.extend_from_slice(&bytes);
    }

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
) -> Result<String, String> {
    if attachment.encrypted {
        let timestamp = Utc::now().timestamp();
        Ok(format!(
            "http://{}.localhost/{}/{}?t={}",
            PROTOCOL_NAME, id, &attachment.filename, timestamp
        ))
    } else {
        let key = remote_attachments_key(id, &attachment.filename);
        let url = client
            .direct_url(&key)
            .map_err(|e| format!("生成附件URL失败:{}", e))?;
        Ok(url)
    }
}
