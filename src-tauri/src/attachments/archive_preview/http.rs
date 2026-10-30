//! 单个解压文件的只读 capability URL。不能由 URL 指定磁盘路径或请求压缩包中的其他条目。
use super::*;
use crate::attachments::archive_preview::entry_command::EntryResource;

pub(super) async fn process(
    state: AppState,
    server: &AttachmentServerHandle,
    global: OwnedSemaphorePermit,
    request: Request<Incoming>,
    view: AttachmentView,
) -> Result<Response<ResponseBody>, ServerError> {
    let id = request
        .uri()
        .path()
        .strip_prefix("/archive-entry/")
        .unwrap_or_default();
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ServerError::NotFound("preview"));
    }
    let resource = server
        .entry_previews
        .get(id)
        .ok_or(ServerError::NotFound("preview expired"))?;
    let permit = server.try_acquire_attachment_request(
        global,
        &resource.target.diary_id,
        &resource.target.attachment_id,
    )?;
    let guard = state
        .lock_storage_operation()
        .await
        .map_err(|_| ServerError::Forbidden("Vault key change is pending"))?;
    if state.is_remote_enabled() != resource.remote {
        return Err(ServerError::NotFound("preview expired"));
    }
    let mimetype = resource.mimetype.as_str();
    if (view == AttachmentView::Html && mimetype != "text/html")
        || (view == AttachmentView::Pdf && mimetype != "application/pdf")
    {
        return Err(ServerError::Forbidden("Preview type is not allowed"));
    }
    if view == AttachmentView::Jsonl {
        if !["jsonl", "ndjson"].iter().any(|extension| {
            resource
                .filename
                .to_lowercase()
                .ends_with(&format!(".{extension}"))
        }) {
            return Err(ServerError::Forbidden("JSONL view is not allowed"));
        }
        if request.method() != Method::GET || request.headers().contains_key(RANGE) {
            return Err(ServerError::BadRequest(
                "JSONL 分页仅支持不带 Range 的 GET 请求",
            ));
        }
        let page = jsonl_preview::preview_source(
            request.uri().query(),
            &resource.id,
            resource.file.length,
            |start, end| {
                let resource = resource.clone();
                async move {
                    resource
                        .read_range(start, (end - start + 1) as usize)
                        .await
                        .map_err(|e| ServerError::Internal(e.to_string()))
                }
            },
        )
        .await?;
        let bytes = serde_json::to_vec(&page).map_err(|e| ServerError::Internal(e.to_string()))?;
        return hold_request_permit(
            build_attachment_response(
                StatusCode::OK,
                "application/json; charset=utf-8",
                None,
                bytes.len() as u64,
                bytes.len() as u64,
                Bytes::from(bytes),
            ),
            permit,
            guard,
        );
    }
    let (offset, size, mime) = match view {
        AttachmentView::MotionPhotoVideo => {
            let header = resource
                .read_range(
                    0,
                    (resource.file.length as usize).min(MOTION_PHOTO_XMP_PROBE_BYTES as usize),
                )
                .await
                .map_err(internal)?;
            let layout = parse_motion_photo_layout(&header, resource.file.length)
                .ok_or(ServerError::NotFound("motion photo"))?;
            let signature = resource
                .read_range(layout.video_start, 16.min(layout.video_length) as usize)
                .await
                .map_err(internal)?;
            if !has_isobmff_file_type_box(&signature) {
                return Err(ServerError::NotFound("motion photo"));
            }
            (layout.video_start, layout.video_length, layout.mime_type)
        }
        AttachmentView::Html => (0, resource.file.length, "text/html; charset=utf-8"),
        _ => (
            0,
            resource.file.length,
            if mimetype == "text/html" {
                "application/octet-stream"
            } else {
                mimetype
            },
        ),
    };
    let raw_range = request
        .headers()
        .get(RANGE)
        .map(|value| {
            value
                .to_str()
                .map_err(|_| ServerError::BadRequest("Invalid Range header"))
        })
        .transpose()?;
    let range = resolve_range(raw_range, size)?;
    let status = if range.is_some() {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let (start, length) = range.map(|(s, e)| (s, e - s + 1)).unwrap_or((0, size));
    let response = if request.method() == Method::HEAD {
        build_attachment_response(status, mime, range, size, length, Bytes::new())
    } else {
        build_attachment_stream_response(
            status,
            mime,
            length,
            resource_stream(resource.clone(), offset + start, length),
        )
        .map(|mut response| {
            if let Some((start, end)) = range {
                response.headers_mut().insert(
                    CONTENT_RANGE,
                    hyper::header::HeaderValue::from_str(&format!("bytes {start}-{end}/{size}"))
                        .expect("numeric range"),
                );
            }
            response
        })
    }
    .map(|mut response| {
        response.headers_mut().insert(
            "x-content-type-options",
            hyper::header::HeaderValue::from_static("nosniff"),
        );
        response
    });
    hold_request_permit(response, permit, guard)
}

fn internal(error: io::Error) -> ServerError {
    ServerError::Internal(error.to_string())
}
fn resource_stream(resource: Arc<EntryResource>, start: u64, length: u64) -> ByteStream {
    Box::pin(futures_util::stream::try_unfold(
        (resource, start, length),
        |(resource, offset, remaining)| async move {
            if remaining == 0 {
                return Ok(None);
            }
            let count = remaining.min(64 * 1024) as usize;
            let bytes = resource.read_range(offset, count).await?;
            Ok(Some((
                Bytes::from(bytes),
                (resource, offset + count as u64, remaining - count as u64),
            )))
        },
    ))
}
