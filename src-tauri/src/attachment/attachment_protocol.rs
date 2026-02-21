use crate::crypto::Crypto;
use crate::diary::{diary_get, DiaryMemoryCache};
use crate::object::OssState;
use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder, Wry};
use crate::crypto::types::EncryptionAlgorithm::Gcm;

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

    let tag = segments[0];
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

    let range_header = request.headers()
        .get(tauri::http::header::RANGE)
        .and_then(|h| h.to_str().ok());

    match tag {
        "image" => {}
        "audio" => {}
        "video" => {}
        _ => {
            responder.respond(bad_request_response());
            return;
        }
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
