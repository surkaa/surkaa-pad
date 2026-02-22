use crate::crypto::types::EncryptionAlgorithm::Gcm;
use crate::crypto::Crypto;
use crate::diary::{diary_get, DiaryMemoryCache};
use crate::object::OssState;
use crate::storage::remote_attachments_key;
use tauri::http::{HeaderMap, Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder, Wry};
use futures_util::StreamExt;

pub fn attachment_protocol(
    context: UriSchemeContext<Wry>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app_handle = context.app_handle();
    let cache = app_handle.state::<DiaryMemoryCache>().inner().clone();
    let crypto = app_handle.state::<Crypto>().inner().clone();
    let oss_state = app_handle.state::<OssState>().inner().clone();
    tauri::async_runtime::spawn(attachment_protocol_inner(
        cache, crypto, oss_state, request, responder,
    ));
}

/// 格式：/tag/id/filename，tag：image audio video
async fn attachment_protocol_inner(
    cache: DiaryMemoryCache,
    crypto: Crypto,
    oss_state: OssState,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let client = match oss_state.get_client() {
        Ok(client) => client,
        Err(_) => {
            responder.respond(error_response("client is not ready".to_string()));
            return;
        }
    };
    let uri = request.uri();
    let path = uri.path();
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    if segments.len() != 3 {
        responder.respond(bad_request_response());
        return;
    }

    let _tag = segments[0];
    let id = segments[1];
    let filename = segments[2];

    let diary = match diary_get(&cache, &crypto, &client, id).await {
        Ok(diary) => diary,
        Err(e) => {
            responder.respond(error_response(e));
            return;
        }
    };
    let attachment = match diary.attachments.iter().find(|a| a.filename == filename) {
        Some(attachment) => attachment,
        None => {
            responder.respond(not_found_response("attachment"));
            return;
        }
    };
    if !attachment.encrypted {
        // 没有加密的应该直接访问云存储
        responder.respond(forbidden_response("encryption is required"));
        return;
    }
    if attachment.algorithm == Gcm {
        // GCM 应该直接使用旧的逻辑 TODO 或者可以在这里下载完整的，然后修改成CTR的方式
        responder.respond(forbidden_response("GCM encryption is not supported"));
        return;
    }

    let range = parse_range_header(request.headers());

    let range = match range {
        HttpRange::Full => (0, attachment.size - 1),
        HttpRange::Range(start, end) => {
            let end = end.unwrap_or(attachment.size - 1);
            if start >= attachment.size || end >= attachment.size || start > end {
                responder.respond(bad_request_response());
                return;
            }
            (start, end)
        }
        HttpRange::Invalid => {
            responder.respond(bad_request_response());
            return;
        }
    };

    let key = remote_attachments_key(id, filename);
    let (stream, len) = match client.download(&key, Some(range)).await {
        Ok((stream, len)) => (stream, len),
        Err(e) => {
            responder.respond(error_response(e));
            return;
        }
    };
    let stream = match crypto.decrypt_streaming(stream, &attachment.nonce) {
        Ok(stream) => stream,
        Err(e) => {
            responder.respond(error_response(e));
            return;
        }
    };

    // 消费 Stream，将其收集到内存中的 Vec<u8>
    // 因为这只是整个文件中的一个 Range Chunk（切片），所以放进内存是安全的
    let mut data = Vec::with_capacity(len as usize);
    let mut pinned_stream = stream;

    while let Some(chunk_result) = pinned_stream.next().await {
        match chunk_result {
            Ok(bytes) => data.extend_from_slice(&bytes),
            Err(e) => {
                responder.respond(error_response(e.to_string()));
                return;
            }
        }
    }

    // 判断前端是否真的发起了 Range 请求
    let is_range_request = request.headers().contains_key(tauri::http::header::RANGE);

    // 构建通用的 Response Header
    let builder = Response::builder()
        .header(tauri::http::header::CONTENT_TYPE, attachment.mimetype.clone())
        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*") // 防止跨域拦截
        .header(tauri::http::header::ACCEPT_RANGES, "bytes");          // 告诉浏览器我们支持进度条拖动

    // 根据请求类型返回 206 或 200
    let response = if is_range_request {
        // range 变量此时是你上面解构出来的 (start, end)
        let (start, end) = range;
        let content_range = format!("bytes {}-{}/{}", start, end, attachment.size);

        builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(tauri::http::header::CONTENT_RANGE, content_range)
            .header(tauri::http::header::CONTENT_LENGTH, data.len().to_string())
            .body(data)
    } else {
        builder
            .status(StatusCode::OK)
            .header(tauri::http::header::CONTENT_LENGTH, data.len().to_string())
            .body(data)
    };

    let response = match response {
        Ok(response) => response,
        Err(e) => {
            responder.respond(error_response(e.to_string()));
            return;
        }
    };

    // 打回给前端 WebView
    responder.respond(response);
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
    let Some(header_val) = headers.get(tauri::http::header::RANGE) else {
        return HttpRange::Full;
    };

    let Ok(range_str) = header_val.to_str() else {
        return HttpRange::Invalid;
    };

    if !range_str.starts_with("bytes=") {
        return HttpRange::Invalid;
    }

    let parts: Vec<&str> = range_str[6..].split('-').collect();
    if parts.is_empty() || parts.len() > 2 {
        return HttpRange::Invalid;
    }

    let start = parts[0].parse::<u64>().unwrap_or(0);

    if parts.len() == 1 || parts[1].is_empty() {
        // 应对 "bytes=500000-" 的情况
        HttpRange::Range(start, None)
    } else if let Ok(end) = parts[1].parse::<u64>() {
        // 应对 "bytes=500000-600000" 的情况
        HttpRange::Range(start, Some(end))
    } else {
        HttpRange::Invalid
    }
}

fn bad_request_response() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(vec![])
        .unwrap()
}

fn forbidden_response(cause: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .body(format!("Forbidden: {}", cause).into_bytes())
        .unwrap()
}

fn not_found_response(target: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(format!("{} not found", target).into_bytes())
        .unwrap()
}

fn error_response(e: String) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(e.into_bytes())
        .unwrap()
}
