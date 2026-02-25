use crate::crypto::types::EncryptionAlgorithm::Gcm;
use crate::crypto::Crypto;
use crate::diary::{get_diary, DiaryMemoryCache};
use crate::object::OssState;
use crate::storage::remote_attachments_key;
use futures_util::StreamExt;
use std::cmp::min;
use tauri::http::header::{
    ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE,
};
use tauri::http::{HeaderMap, Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder, Wry};
use tauri_plugin_log::log;

const MAX_CHUNK_SIZE: u64 = 1024 * 1024; // 1MB

/// 统一协议错误路由
#[derive(Debug)]
enum ProtocolError {
    BadRequest(&'static str),
    Forbidden(&'static str),
    NotFound(&'static str),
    Internal(String),
}

impl ProtocolError {
    fn into_response(self) -> Response<Vec<u8>> {
        let (status, msg) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m.to_string()),
            Self::Forbidden(m) => (StatusCode::FORBIDDEN, m.to_string()),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, format!("{} not found", m)),
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

/// 格式：/tag/id/filename，tag：image audio video
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
    let [_tag, id, filename] = segments.as_slice() else {
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
        return Err(ProtocolError::Forbidden(
            "GCM decryption is not supported",
        ));
    }

    let range_header = parse_range_header(request.headers());
    let (start, end) = match range_header {
        HttpRange::Full => (0, attachment.size.saturating_sub(1)),
        HttpRange::Range(s, e) => {
            let e = e.unwrap_or_else(|| attachment.size.saturating_sub(1));
            if s >= attachment.size || e >= attachment.size || s > e {
                return Err(ProtocolError::BadRequest("Invalid Range headers"));
            }
            (s, min(e, s + MAX_CHUNK_SIZE - 1))
        }
        HttpRange::Invalid => return Err(ProtocolError::BadRequest("Invalid Range format")),
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

    let is_range = request.headers().contains_key(RANGE);
    let status = if is_range {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };

    let mut builder = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, attachment.mimetype.clone())
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(ACCEPT_RANGES, "bytes")
        .header(CONTENT_LENGTH, data.len().to_string());

    if is_range {
        builder = builder.header(
            CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, attachment.size),
        );
    }

    builder
        .body(data)
        .map_err(|e| ProtocolError::Internal(e.to_string()))
}

pub enum HttpRange {
    /// 没有 Range 头（比如图片），返回全量数据 200 OK
    Full,
    /// 包含合法的 Range 头，返回 206 Partial Content
    /// (起始字节, 结束字节: 若浏览器未指定则为 None)
    Range(u64, Option<u64>),
    /// Range 头格式错误，应返回 416 Range Not Satisfiable 或兜底返回 Full
    Invalid,
}

pub fn parse_range_header(headers: &HeaderMap) -> HttpRange {
    let Some(range_str) = headers
        .get(RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("bytes="))
    else {
        return HttpRange::Full; // 无效头兜底为 Full
    };

    let mut parts = range_str.splitn(2, '-');
    let start_str = parts.next().unwrap_or("");
    let end_str = parts.next().unwrap_or("");

    if start_str.is_empty() {
        return HttpRange::Invalid;
    }

    let Ok(start) = start_str.parse::<u64>() else {
        return HttpRange::Invalid;
    };

    if end_str.is_empty() {
        HttpRange::Range(start, None)
    } else if let Ok(end) = end_str.parse::<u64>() {
        HttpRange::Range(start, Some(end))
    } else {
        HttpRange::Invalid
    }
}
