use crate::attachments::embedded_media::{
    has_isobmff_file_type_box, parse_motion_photo_layout, MotionPhotoLayout,
    MOTION_PHOTO_XMP_PROBE_BYTES,
};
use crate::attachments::{AttachmentError, AttachmentMeta};
use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
use crate::cryptos::{Crypto, CryptoError};
use crate::diaries::{get_diary, DiaryError, DiaryStore};
use crate::object::ObjectError;
use crate::state::AppState;
use crate::stream::{collect_data_with_capacity, ByteStream};
use bytes::Bytes;
use dashmap::DashMap;
use futures_util::TryStreamExt;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full, StreamBody};
use http_range_header::parse_range_header;
use hyper::body::Frame;
use hyper::body::Incoming;
use hyper::header::{
    ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, CACHE_CONTROL, CONTENT_LENGTH,
    CONTENT_RANGE, CONTENT_TYPE, RANGE,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::cmp::min;
use std::convert::Infallible;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri_plugin_log::log;

const MAX_CHUNK_SIZE: u64 = 1024 * 1024;
const MOTION_PHOTO_VIEW: &str = "motion-photo-video";
type ResponseBody = UnsyncBoxBody<Bytes, io::Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachmentView {
    Original,
    MotionPhotoVideo,
}

#[derive(Clone, Copy, Debug)]
enum CachedMotionPhoto {
    NotMotionPhoto,
    MotionPhoto(MotionPhotoLayout),
}

#[derive(Clone, Debug)]
struct MotionPhotoCacheEntry {
    etag: String,
    object_size: u64,
    value: CachedMotionPhoto,
}

#[derive(Clone, Debug)]
pub struct AttachmentServerHandle {
    origin: Arc<str>,
    token: Arc<str>,
    motion_photo_cache: Arc<DashMap<(String, String), MotionPhotoCacheEntry>>,
}

impl AttachmentServerHandle {
    fn new(address: SocketAddr, token: String) -> Self {
        Self {
            origin: format!("http://{}", address).into(),
            token: token.into(),
            motion_photo_cache: Arc::new(DashMap::new()),
        }
    }

    pub fn url(&self, diary_id: &str, attachment_id: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!(
            "{}/{}/{}/{}?t={}",
            self.origin,
            self.token,
            urlencoding::encode(diary_id),
            urlencoding::encode(attachment_id),
            timestamp
        )
    }

    fn token(&self) -> Arc<str> {
        self.token.clone()
    }

    #[cfg(test)]
    pub fn for_test() -> Self {
        Self {
            origin: "http://127.0.0.1:1".into(),
            token: "test-token".into(),
            motion_photo_cache: Arc::new(DashMap::new()),
        }
    }
}

pub fn bind_attachment_server() -> io::Result<(TcpListener, AttachmentServerHandle)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;

    let mut token = [0_u8; 32];
    getrandom::fill(&mut token).map_err(|error| io::Error::other(error.to_string()))?;
    let handle = AttachmentServerHandle::new(listener.local_addr()?, hex::encode(token));
    Ok((listener, handle))
}

pub fn start_attachment_server(listener: TcpListener, state: AppState) {
    tauri::async_runtime::spawn(run_attachment_server(listener, state));
}

async fn run_attachment_server(listener: TcpListener, state: AppState) {
    let token = state.attachment_server().token();
    let address = match listener.local_addr() {
        Ok(address) => address,
        Err(error) => {
            log::error!("无法读取附件 HTTP 服务地址: {error}");
            return;
        }
    };
    let listener = match tokio::net::TcpListener::from_std(listener) {
        Ok(listener) => listener,
        Err(error) => {
            log::error!("无法启动附件 HTTP 服务: {error}");
            return;
        }
    };
    log::info!("附件 HTTP 服务已监听 {address}");

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(connection) => connection,
            Err(error) => {
                log::error!("附件 HTTP 服务接受连接失败: {error}");
                continue;
            }
        };
        let state = state.clone();
        let token = token.clone();
        tokio::spawn(async move {
            let service =
                service_fn(move |request| handle_request(state.clone(), token.clone(), request));
            if let Err(error) = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                log::debug!("附件 HTTP 连接已结束: {error}");
            }
        });
    }
}

async fn handle_request(
    state: AppState,
    token: Arc<str>,
    request: Request<Incoming>,
) -> Result<Response<ResponseBody>, Infallible> {
    let response = process_attachment(state, &token, request)
        .await
        .unwrap_or_else(|error| {
            log::error!("附件 HTTP 请求失败: {error:?}");
            error.into_response()
        });
    Ok(response)
}

#[derive(Debug)]
enum ServerError {
    BadRequest(&'static str),
    Forbidden(&'static str),
    NotFound(&'static str),
    MethodNotAllowed,
    RangeNotSatisfiable(u64),
    Internal(String),
}

impl ServerError {
    fn into_response(self) -> Response<ResponseBody> {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message.to_string()),
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, message.to_string()),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, format!("{message} not found")),
            Self::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                "Method not allowed".to_string(),
            ),
            Self::RangeNotSatisfiable(size) => {
                return response_builder_with_status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(CONTENT_RANGE, format!("bytes */{size}"))
                    .body(full_body(Bytes::new()))
                    .unwrap_or_else(|_| Response::new(full_body(Bytes::new())));
            }
            Self::Internal(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
        };
        response_builder_with_status(status)
            .body(full_body(Bytes::from(message)))
            .unwrap_or_else(|_| Response::new(full_body(Bytes::new())))
    }
}

impl From<DiaryError> for ServerError {
    fn from(error: DiaryError) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<CryptoError> for ServerError {
    fn from(error: CryptoError) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<AttachmentError> for ServerError {
    fn from(error: AttachmentError) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<ObjectError> for ServerError {
    fn from(error: ObjectError) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<io::Error> for ServerError {
    fn from(error: io::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

fn response_builder() -> hyper::http::response::Builder {
    Response::builder()
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(ACCESS_CONTROL_ALLOW_HEADERS, "Range")
        .header(ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
        .header(
            ACCESS_CONTROL_EXPOSE_HEADERS,
            "Accept-Ranges, Content-Length, Content-Range",
        )
        .header(CACHE_CONTROL, "no-store")
}

fn response_builder_with_status(status: StatusCode) -> hyper::http::response::Builder {
    response_builder().status(status)
}

async fn process_attachment(
    state: AppState,
    expected_token: &str,
    request: Request<Incoming>,
) -> Result<Response<ResponseBody>, ServerError> {
    if request.method() == Method::OPTIONS {
        return response_builder_with_status(StatusCode::NO_CONTENT)
            .body(full_body(Bytes::new()))
            .map_err(|error| ServerError::Internal(error.to_string()));
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return Err(ServerError::MethodNotAllowed);
    }
    let attachment_view = parse_attachment_view(request.uri().query())?;

    let path = request.uri().path().trim_start_matches('/');
    let segments: Vec<&str> = path.split('/').collect();
    let [token, encoded_id, encoded_attachment_id] = segments.as_slice() else {
        return Err(ServerError::BadRequest("Invalid URI path structure"));
    };
    if *token != expected_token {
        return Err(ServerError::NotFound("attachment"));
    }

    let id = urlencoding::decode(encoded_id)
        .map_err(|_| ServerError::BadRequest("Invalid URL encoding in diary id"))?
        .into_owned();
    let attachment_id = urlencoding::decode(encoded_attachment_id)
        .map_err(|_| ServerError::BadRequest("Invalid URL encoding in attachment id"))?
        .into_owned();

    // 整个 HTTP 请求固定使用进入时的存储模式，避免读取 Manifest 后切换到另一存储。
    let _storage_guard = state.lock_storage_operation().await;
    let cache = state.diary_cache();
    let crypto = state.crypto();
    let store = state.diary_store();
    let diary = match cache.get(&id) {
        Some((diary, _)) => diary,
        None => get_diary(&cache, &crypto, &*store, &id).await?,
    };
    let attachment = diary
        .attachments
        .iter()
        .find(|attachment| attachment.id == attachment_id)
        .ok_or(ServerError::NotFound("attachment"))?;
    if attachment.algorithm == Gcm {
        return Err(ServerError::Forbidden("GCM decryption is not supported"));
    }

    let raw_range = request
        .headers()
        .get(RANGE)
        .map(|value| {
            value
                .to_str()
                .map_err(|_| ServerError::BadRequest("Invalid Range header"))
        })
        .transpose()?;
    // Range、HEAD 和内嵌媒体响应必须先取得真实对象长度。普通图片 GET 继续使用
    // Manifest 长度，避免图片列表加载额外增加一次远端 HEAD。
    let mut file_size = if raw_range.is_some()
        || request.method() == Method::HEAD
        || attachment_view == AttachmentView::MotionPhotoVideo
    {
        store
            .get_attachment_size(&id, &attachment_id, attachment.etag.as_deref())
            .await?
    } else {
        attachment.size
    };
    let (resource_offset, resource_size, resource_mimetype, is_virtual) = match attachment_view {
        AttachmentView::Original => (0, file_size, attachment.mimetype.as_str(), false),
        AttachmentView::MotionPhotoVideo => {
            let layout = resolve_motion_photo_layout(
                &state.attachment_server(),
                &*store,
                &crypto,
                &id,
                attachment,
                file_size,
            )
            .await?;
            (
                layout.video_start,
                layout.video_length,
                layout.mime_type,
                true,
            )
        }
    };
    let range = resolve_range(raw_range, resource_size)?;
    let selected_length = range
        .map(|(start, end)| end.saturating_sub(start) + 1)
        .unwrap_or(resource_size);
    let status = if range.is_some() {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };

    if request.method() == Method::HEAD {
        return build_attachment_response(
            status,
            resource_mimetype,
            range,
            resource_size,
            selected_length,
            Bytes::new(),
        );
    }

    let physical_range = if is_virtual {
        let (start, end) = range.unwrap_or((0, resource_size.saturating_sub(1)));
        Some((
            resource_offset
                .checked_add(start)
                .ok_or(ServerError::Internal(
                    "Embedded media range overflow".into(),
                ))?,
            resource_offset
                .checked_add(end)
                .ok_or(ServerError::Internal(
                    "Embedded media range overflow".into(),
                ))?,
        ))
    } else {
        range
    };
    let stream = store
        .download_attachment(
            &id,
            &attachment_id,
            physical_range,
            attachment.etag.as_deref(),
        )
        .await?;
    let start = physical_range.map(|(start, _)| start).unwrap_or(0);
    let stream = if attachment.encrypted {
        crypto.decrypt_streaming(stream, &attachment.nonce, start)?
    } else {
        stream
    };
    if is_virtual && range.is_none() {
        return build_attachment_stream_response(
            status,
            resource_mimetype,
            selected_length,
            stream,
        );
    }
    let capacity = min(selected_length, MAX_CHUNK_SIZE) as usize;
    let data = collect_data_with_capacity(stream, capacity).await?;
    let actual_length = data.len() as u64;
    if (is_virtual || range.is_some()) && actual_length != selected_length {
        return Err(ServerError::Internal(format!(
            "Attachment length mismatch: expected {selected_length}, got {actual_length}"
        )));
    }
    if !is_virtual && range.is_none() && actual_length != file_size {
        log::warn!(
            "附件 Manifest 长度与对象不一致，使用对象实际长度: diary_id={}, attachment_id={}, manifest_size={}, actual_size={}",
            id,
            attachment_id,
            file_size,
            actual_length
        );
        file_size = actual_length;
    }
    let actual_range = range.map(|(start, _)| {
        let end = start.saturating_add(actual_length.saturating_sub(1));
        (start, end)
    });

    build_attachment_response(
        status,
        resource_mimetype,
        actual_range,
        if is_virtual { resource_size } else { file_size },
        actual_length,
        Bytes::from(data),
    )
}

fn parse_attachment_view(query: Option<&str>) -> Result<AttachmentView, ServerError> {
    let mut requested_view = None;
    for component in query.unwrap_or_default().split('&') {
        let Some((name, value)) = component.split_once('=') else {
            continue;
        };
        if name != "view" {
            continue;
        }
        if requested_view.replace(value).is_some() {
            return Err(ServerError::BadRequest("Duplicate attachment view"));
        }
    }

    match requested_view {
        None => Ok(AttachmentView::Original),
        Some(MOTION_PHOTO_VIEW) => Ok(AttachmentView::MotionPhotoVideo),
        Some(_) => Err(ServerError::BadRequest("Unsupported attachment view")),
    }
}

async fn resolve_motion_photo_layout(
    server: &AttachmentServerHandle,
    store: &dyn DiaryStore,
    crypto: &Crypto,
    diary_id: &str,
    attachment: &AttachmentMeta,
    object_size: u64,
) -> Result<MotionPhotoLayout, ServerError> {
    let cache_key = (diary_id.to_owned(), attachment.id.clone());
    if let Some(etag) = attachment.etag.as_deref() {
        if let Some(entry) = server.motion_photo_cache.get(&cache_key) {
            if entry.etag == etag && entry.object_size == object_size {
                return match entry.value {
                    CachedMotionPhoto::MotionPhoto(layout) => Ok(layout),
                    CachedMotionPhoto::NotMotionPhoto => {
                        Err(ServerError::NotFound("motion photo video"))
                    }
                };
            }
        }
    }

    let value =
        detect_motion_photo_layout(store, crypto, diary_id, attachment, object_size).await?;
    if let Some(etag) = attachment.etag.as_deref() {
        server.motion_photo_cache.insert(
            cache_key,
            MotionPhotoCacheEntry {
                etag: etag.to_owned(),
                object_size,
                value,
            },
        );
    }

    match value {
        CachedMotionPhoto::MotionPhoto(layout) => Ok(layout),
        CachedMotionPhoto::NotMotionPhoto => Err(ServerError::NotFound("motion photo video")),
    }
}

async fn detect_motion_photo_layout(
    store: &dyn DiaryStore,
    crypto: &Crypto,
    diary_id: &str,
    attachment: &AttachmentMeta,
    object_size: u64,
) -> Result<CachedMotionPhoto, ServerError> {
    if object_size == 0 {
        return Ok(CachedMotionPhoto::NotMotionPhoto);
    }
    let probe_end = min(object_size, MOTION_PHOTO_XMP_PROBE_BYTES) - 1;
    let header =
        read_plain_attachment_range(store, crypto, diary_id, attachment, (0, probe_end)).await?;
    let Some(layout) = parse_motion_photo_layout(&header, object_size) else {
        return Ok(CachedMotionPhoto::NotMotionPhoto);
    };

    let signature_end = min(
        layout.video_start.saturating_add(11),
        object_size.saturating_sub(1),
    );
    let signature = read_plain_attachment_range(
        store,
        crypto,
        diary_id,
        attachment,
        (layout.video_start, signature_end),
    )
    .await?;
    if !has_isobmff_file_type_box(&signature) {
        return Ok(CachedMotionPhoto::NotMotionPhoto);
    }
    Ok(CachedMotionPhoto::MotionPhoto(layout))
}

async fn read_plain_attachment_range(
    store: &dyn DiaryStore,
    crypto: &Crypto,
    diary_id: &str,
    attachment: &AttachmentMeta,
    range: (u64, u64),
) -> Result<Vec<u8>, ServerError> {
    let expected_length = range.1.saturating_sub(range.0) + 1;
    let stream = store
        .download_attachment(
            diary_id,
            &attachment.id,
            Some(range),
            attachment.etag.as_deref(),
        )
        .await?;
    let stream = if attachment.encrypted {
        crypto.decrypt_streaming(stream, &attachment.nonce, range.0)?
    } else {
        stream
    };
    let data =
        collect_data_with_capacity(stream, min(expected_length, MAX_CHUNK_SIZE) as usize).await?;
    if data.len() as u64 != expected_length {
        return Err(ServerError::Internal(format!(
            "Attachment length mismatch: expected {expected_length}, got {}",
            data.len()
        )));
    }
    Ok(data)
}

fn resolve_range(
    raw_range: Option<&str>,
    file_size: u64,
) -> Result<Option<(u64, u64)>, ServerError> {
    let Some(raw_range) = raw_range else {
        return Ok(None);
    };
    let ranges = parse_range_header(raw_range)
        .map_err(|_| ServerError::BadRequest("Invalid Range format"))?;
    let valid_ranges = ranges
        .validate(file_size)
        .map_err(|_| ServerError::RangeNotSatisfiable(file_size))?;
    let range = valid_ranges
        .first()
        .ok_or(ServerError::RangeNotSatisfiable(file_size))?;
    let start = *range.start();
    let end = min(*range.end(), start.saturating_add(MAX_CHUNK_SIZE - 1));
    Ok(Some((start, end)))
}

fn build_attachment_response(
    status: StatusCode,
    mimetype: &str,
    range: Option<(u64, u64)>,
    file_size: u64,
    content_length: u64,
    body: Bytes,
) -> Result<Response<ResponseBody>, ServerError> {
    let mut builder = response_builder_with_status(status)
        .header(CONTENT_TYPE, mimetype)
        .header(ACCEPT_RANGES, "bytes")
        .header(CONTENT_LENGTH, content_length);
    if let Some((start, end)) = range {
        builder = builder.header(CONTENT_RANGE, format!("bytes {start}-{end}/{file_size}"));
    }
    builder
        .body(full_body(body))
        .map_err(|error| ServerError::Internal(error.to_string()))
}

fn build_attachment_stream_response(
    status: StatusCode,
    mimetype: &str,
    content_length: u64,
    stream: ByteStream,
) -> Result<Response<ResponseBody>, ServerError> {
    response_builder_with_status(status)
        .header(CONTENT_TYPE, mimetype)
        .header(ACCEPT_RANGES, "bytes")
        .header(CONTENT_LENGTH, content_length)
        .body(stream_body(stream))
        .map_err(|error| ServerError::Internal(error.to_string()))
}

fn full_body(body: Bytes) -> ResponseBody {
    Full::new(body)
        .map_err(|never: Infallible| match never {})
        .boxed_unsync()
}

fn stream_body(stream: ByteStream) -> ResponseBody {
    StreamBody::new(stream.map_ok(Frame::data)).boxed_unsync()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachments::AttachmentMeta;
    use crate::caches::LocalObjectStore;
    use crate::cryptos::crypto_types::EncryptionAlgorithm::{Ctr, Gcm};
    use crate::cryptos::Crypto;
    use crate::diaries::{DiaryContent, DiaryManifest, DiaryStore, LocalStore, CURRENT_VERSION};
    use crate::object::OssClient;
    use crate::stream::{collect_data, create_mock_stream};
    use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
    use tempfile::TempDir;

    struct TestServer {
        _temp_dir: TempDir,
        handle: AttachmentServerHandle,
        state: AppState,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn start_test_server(
        id: &str,
        filename: &str,
        mimetype: &str,
        plaintext: &[u8],
        encrypted: bool,
    ) -> TestServer {
        start_test_server_with_declared_size(
            id,
            filename,
            mimetype,
            plaintext,
            encrypted,
            plaintext.len() as u64,
        )
        .await
    }

    async fn start_test_server_with_declared_size(
        id: &str,
        filename: &str,
        mimetype: &str,
        plaintext: &[u8],
        encrypted: bool,
        declared_size: u64,
    ) -> TestServer {
        let temp_dir = tempfile::tempdir().unwrap();
        let local_object_store = LocalObjectStore::new(temp_dir.path().to_path_buf());
        let crypto = Crypto::new();
        crypto.init_by_dek_string("42".repeat(32)).unwrap();
        let store = LocalStore::new(local_object_store.clone());

        let source = create_mock_stream(plaintext.to_vec(), 64 * 1024);
        let (stream, nonce) = if encrypted {
            crypto.encrypt_streaming(source).unwrap()
        } else {
            (source, Vec::new())
        };
        let etag = store
            .upload_attachment(id, filename, plaintext.len() as u64, mimetype, stream)
            .await
            .unwrap();

        let (listener, handle) = bind_attachment_server().unwrap();
        let state = AppState::from_parts_with_attachment_server(
            crypto,
            OssClient::new(),
            local_object_store,
            handle.clone(),
        );
        state.diary_cache().insert(
            id,
            DiaryManifest {
                id: id.to_string(),
                algorithm: Gcm,
                content: DiaryContent::default(),
                created: 0,
                updated: 0,
                attachments: vec![AttachmentMeta {
                    id: filename.to_string(),
                    filename: filename.to_string(),
                    mimetype: mimetype.to_string(),
                    size: declared_size,
                    encrypted,
                    nonce,
                    algorithm: Ctr,
                    etag: Some(etag),
                    content_info: None,
                }],
                version: CURRENT_VERSION,
            },
            "manifest-etag".to_string(),
        );
        let task = tokio::spawn(run_attachment_server(listener, state.clone()));

        TestServer {
            _temp_dir: temp_dir,
            handle,
            state,
            task,
        }
    }

    fn motion_photo_fixture(video_length: usize) -> (Vec<u8>, Vec<u8>) {
        let image_length = 192 * 1024;
        let mut video: Vec<u8> = (0..video_length).map(|index| (index % 251) as u8).collect();
        video[..12].copy_from_slice(b"\0\0\0\x18ftypmp42");

        let mut attachment = vec![0_u8; image_length + video_length];
        let xmp = format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
              <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
                xmlns:Camera="http://ns.google.com/photos/1.0/camera/">
                <rdf:Description Camera:MicroVideo="1"
                  Camera:MicroVideoOffset="{video_length}"/>
              </rdf:RDF>
            </x:xmpmeta>"#
        );
        attachment[256..256 + xmp.len()].copy_from_slice(xmp.as_bytes());
        attachment[image_length..].copy_from_slice(&video);
        (attachment, video)
    }

    fn motion_photo_video_url(attachment_url: &str) -> String {
        format!("{attachment_url}&view={MOTION_PHOTO_VIEW}")
    }

    #[test]
    fn generated_url_is_loopback_only_and_encodes_path_segments() {
        let (listener, handle) = bind_attachment_server().unwrap();
        drop(listener);

        let url = handle.url("diary id", "中文 image #1.jpg");
        assert!(url.starts_with("http://127.0.0.1:"));
        assert!(url.contains("/diary%20id/"));
        assert!(url.contains("%E4%B8%AD%E6%96%87%20image%20%231.jpg"));
        assert!(!url.contains("test-token"));
    }

    #[test]
    fn range_resolution_caps_open_ended_requests() {
        assert_eq!(resolve_range(None, 123).unwrap(), None);
        assert_eq!(
            resolve_range(Some("bytes=10-19"), 100).unwrap(),
            Some((10, 19))
        );
        assert_eq!(
            resolve_range(Some("bytes=5-"), MAX_CHUNK_SIZE * 2).unwrap(),
            Some((5, MAX_CHUNK_SIZE + 4))
        );
        assert!(matches!(
            resolve_range(Some("bytes=100-"), 100),
            Err(ServerError::RangeNotSatisfiable(100))
        ));
        assert!(matches!(
            resolve_range(Some("not-a-range"), 100),
            Err(ServerError::BadRequest(_))
        ));
    }

    #[test]
    fn parses_attachment_view_without_interfering_with_cache_buster() {
        assert_eq!(
            parse_attachment_view(Some("t=123&view=motion-photo-video")).unwrap(),
            AttachmentView::MotionPhotoVideo
        );
        assert_eq!(
            parse_attachment_view(Some("t=123")).unwrap(),
            AttachmentView::Original
        );
        assert!(matches!(
            parse_attachment_view(Some("view=unknown")),
            Err(ServerError::BadRequest(_))
        ));
        assert!(matches!(
            parse_attachment_view(Some("view=motion-photo-video&view=motion-photo-video")),
            Err(ServerError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn serves_motion_photo_video_ranges_for_plain_and_encrypted_attachments() {
        let (attachment, video) = motion_photo_fixture(1_700_000);
        let client = reqwest::Client::new();

        for encrypted in [false, true] {
            let server = start_test_server(
                if encrypted {
                    "motion-ctr"
                } else {
                    "motion-plain"
                },
                "motion.jpg",
                "image/jpeg",
                &attachment,
                encrypted,
            )
            .await;
            let url = motion_photo_video_url(&server.handle.url(
                if encrypted {
                    "motion-ctr"
                } else {
                    "motion-plain"
                },
                "motion.jpg",
            ));

            let response = client.head(&url).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[CONTENT_TYPE], "video/mp4");
            assert_eq!(response.headers()[CONTENT_LENGTH], video.len().to_string());

            let response = client
                .get(&url)
                .header(RANGE, "bytes=12345-13344")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(
                response.headers()[CONTENT_RANGE],
                format!("bytes 12345-13344/{}", video.len())
            );
            assert_eq!(
                response.bytes().await.unwrap().as_ref(),
                &video[12_345..13_345]
            );
        }
    }

    #[tokio::test]
    async fn streams_motion_photo_video_without_an_initial_range_request() {
        let (attachment, video) = motion_photo_fixture(MAX_CHUNK_SIZE as usize + 97);
        let server = start_test_server(
            "motion-capped",
            "motion.jpg",
            "image/jpeg",
            &attachment,
            false,
        )
        .await;
        let url = motion_photo_video_url(&server.handle.url("motion-capped", "motion.jpg"));

        let response = reqwest::get(url).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_LENGTH], video.len().to_string());
        assert_eq!(response.bytes().await.unwrap().as_ref(), video);
    }

    #[tokio::test]
    async fn rejects_residual_motion_photo_metadata_without_a_video_signature() {
        let (mut attachment, _) = motion_photo_fixture(4096);
        let video_start = attachment.len() - 4096;
        attachment[video_start..video_start + 12].fill(0);
        let server = start_test_server(
            "motion-residual",
            "edited.jpg",
            "image/jpeg",
            &attachment,
            false,
        )
        .await;
        let url = motion_photo_video_url(&server.handle.url("motion-residual", "edited.jpg"));

        let response = reqwest::get(url).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn serves_plain_attachment_and_head_metadata() {
        let data = b"plain attachment payload";
        let server = start_test_server("diary-1", "photo one.jpg", "image/jpeg", data, false).await;
        let url = server.handle.url("diary-1", "photo one.jpg");
        let client = reqwest::Client::new();

        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_LENGTH], data.len().to_string());
        assert_eq!(response.bytes().await.unwrap().as_ref(), data);

        let response = client.head(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_LENGTH], data.len().to_string());
        assert!(response.bytes().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn attachment_request_waits_for_storage_mode_transition() {
        let data = b"mode-stable attachment";
        let server = start_test_server("diary-gate", "file.txt", "text/plain", data, false).await;
        let transition_guard = server
            .state
            .try_lock_storage_mode_change()
            .expect("应能开始存储模式切换");
        let url = server.handle.url("diary-gate", "file.txt");
        let mut request = tokio::spawn(async move { reqwest::get(url).await });

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut request)
                .await
                .is_err()
        );

        drop(transition_guard);
        let response = tokio::time::timeout(std::time::Duration::from_secs(2), request)
            .await
            .expect("释放模式切换锁后请求应继续")
            .expect("请求任务不应 panic")
            .expect("请求应成功");
        assert_eq!(response.bytes().await.unwrap().as_ref(), data);
    }

    #[tokio::test]
    async fn decrypts_encrypted_attachment_ranges_and_caps_response_size() {
        let data: Vec<u8> = (0..(MAX_CHUNK_SIZE * 2 + 97))
            .map(|index| (index % 251) as u8)
            .collect();
        let server = start_test_server("diary-2", "video.mp4", "video/mp4", &data, true).await;
        let url = server.handle.url("diary-2", "video.mp4");
        let start = 12_345_u64;

        let response = reqwest::Client::new()
            .get(url)
            .header(RANGE, format!("bytes={start}-"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        let expected_end = start + MAX_CHUNK_SIZE - 1;
        assert_eq!(
            response.headers()[CONTENT_RANGE],
            format!("bytes {start}-{expected_end}/{}", data.len())
        );
        assert_eq!(
            response.headers()[CONTENT_LENGTH],
            MAX_CHUNK_SIZE.to_string()
        );
        let body = response.bytes().await.unwrap();
        assert_eq!(body.as_ref(), &data[start as usize..=expected_end as usize]);
    }

    #[tokio::test]
    async fn serves_legacy_attachment_when_manifest_size_is_sixteen_bytes_too_large() {
        let data = b"legacy encrypted attachment payload";
        let server = start_test_server_with_declared_size(
            "legacy-diary",
            "legacy-audio.webm",
            "audio/webm",
            data,
            true,
            data.len() as u64 + 16,
        )
        .await;
        let url = server.handle.url("legacy-diary", "legacy-audio.webm");
        let client = reqwest::Client::new();

        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_LENGTH], data.len().to_string());
        assert_eq!(response.bytes().await.unwrap().as_ref(), data);

        let response = client.head(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_LENGTH], data.len().to_string());

        let suffix_length = 7;
        let response = client
            .get(&url)
            .header(RANGE, format!("bytes=-{suffix_length}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers()[CONTENT_RANGE],
            format!(
                "bytes {}-{}/{}",
                data.len() - suffix_length,
                data.len() - 1,
                data.len()
            )
        );
        assert_eq!(
            response.bytes().await.unwrap().as_ref(),
            &data[data.len() - suffix_length..]
        );
    }

    #[tokio::test]
    async fn rejects_invalid_token_and_unsatisfiable_range() {
        let data = b"short";
        let server = start_test_server("diary-3", "audio.mp3", "audio/mpeg", data, true).await;
        let valid_url = server.handle.url("diary-3", "audio.mp3");
        let invalid_url = valid_url.replacen(server.handle.token.as_ref(), "wrong-token", 1);
        let client = reqwest::Client::new();

        let response = client.get(invalid_url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = client
            .get(valid_url)
            .header(RANGE, format!("bytes={}-", data.len()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(
            response.headers()[CONTENT_RANGE],
            format!("bytes */{}", data.len())
        );
    }

    #[tokio::test]
    async fn encrypted_test_fixture_roundtrips_before_serving() {
        let crypto = Crypto::new();
        crypto.init_by_dek_string("24".repeat(32)).unwrap();
        let data = b"fixture";
        let (encrypted, nonce) = crypto
            .encrypt_streaming(create_mock_stream(data.to_vec(), 2))
            .unwrap();
        let encrypted = collect_data(encrypted).await.unwrap();
        let decrypted = crypto
            .decrypt_streaming(create_mock_stream(encrypted, 3), &nonce, 0)
            .unwrap();
        assert_eq!(collect_data(decrypted).await.unwrap(), data);
    }
}
