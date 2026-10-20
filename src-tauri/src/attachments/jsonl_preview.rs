use super::{read_plain_attachment_range, ServerError};
use crate::attachments::AttachmentMeta;
use crate::cryptos::Crypto;
use crate::diaries::DiaryStore;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::time::Duration;

const PAGE_LINES: usize = 10;
const READ_CHUNK_BYTES: u64 = 64 * 1024;
const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
const LINE_TOO_LARGE: &str = "JSONL 单行超过 4 MiB 安全上限，请下载后查看";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JsonlLine {
    line_number: u64,
    text: String,
    error: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JsonlPage {
    lines: Vec<JsonlLine>,
    next_cursor: Option<String>,
}

// 游标只在本次预览中使用；记录字节位置，后续页不再从头扫描文件。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    offset: u64,
    line_number: u64,
    size: u64,
    etag: Option<String>,
    diary_id: String,
    attachment_id: String,
}

pub(super) fn is_jsonl_attachment(attachment: &AttachmentMeta) -> bool {
    let filename = attachment.filename.trim().to_ascii_lowercase();
    let mimetype = attachment.mimetype.split(';').next().unwrap_or("").trim();
    filename.ends_with(".jsonl")
        || filename.ends_with(".ndjson")
        || [
            "application/jsonl",
            "application/x-jsonlines",
            "application/x-ndjson",
            "application/ndjson",
            "text/jsonl",
            "text/x-jsonl",
        ]
        .iter()
        .any(|value| mimetype.eq_ignore_ascii_case(value))
}

pub(super) async fn preview_jsonl(
    store: &dyn DiaryStore,
    crypto: &Crypto,
    diary_id: &str,
    attachment: &AttachmentMeta,
    size: u64,
    query: Option<&str>,
) -> Result<JsonlPage, ServerError> {
    let cursor = parse_cursor(query, diary_id, attachment, size)?;
    tokio::time::timeout(
        Duration::from_secs(30),
        read_page(cursor, |start, end| {
            read_plain_attachment_range(store, crypto, diary_id, attachment, (start, end))
        }),
    )
    .await
    .map_err(|_| ServerError::BadRequest("读取 JSONL 超时，请稍后重试"))?
}

fn parse_cursor(
    query: Option<&str>,
    diary_id: &str,
    attachment: &AttachmentMeta,
    size: u64,
) -> Result<Cursor, ServerError> {
    let mut encoded = None;
    for component in query.unwrap_or_default().split('&') {
        if let Some(("cursor", value)) = component.split_once('=') {
            if encoded.replace(value).is_some() {
                return Err(ServerError::BadRequest("重复的 JSONL 游标"));
            }
        }
    }
    let Some(encoded) = encoded else {
        return Ok(Cursor {
            offset: 0,
            line_number: 1,
            size,
            etag: attachment.etag.clone(),
            diary_id: diary_id.into(),
            attachment_id: attachment.id.clone(),
        });
    };
    if encoded.len() > 4096 {
        return Err(ServerError::BadRequest("无效的 JSONL 游标"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| ServerError::BadRequest("无效的 JSONL 游标"))?;
    let cursor: Cursor =
        serde_json::from_slice(&bytes).map_err(|_| ServerError::BadRequest("无效的 JSONL 游标"))?;
    if cursor.size != size
        || cursor.etag != attachment.etag
        || cursor.diary_id != diary_id
        || cursor.attachment_id != attachment.id
    {
        return Err(ServerError::BadRequest("附件已变化，请重新打开 JSONL 预览"));
    }
    if cursor.offset > size
        || cursor.line_number == 0
        || cursor.line_number > cursor.offset.saturating_add(1)
    {
        return Err(ServerError::BadRequest("JSONL 游标超出有效范围"));
    }
    Ok(cursor)
}

async fn read_page<F, Fut>(mut cursor: Cursor, mut read: F) -> Result<JsonlPage, ServerError>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, ServerError>>,
{
    let mut lines = Vec::new();
    let mut line = Vec::new();
    let mut line_start = cursor.offset;
    let mut page_bytes = 0;
    let mut first_chunk = true;
    while cursor.offset < cursor.size {
        // 从前一字节开始读取以确认游标确实落在换行后；最多多读一个字节。
        let check_boundary = first_chunk && cursor.offset > 0;
        let start = cursor.offset - u64::from(check_boundary);
        let end = cursor
            .offset
            .saturating_add(READ_CHUNK_BYTES - 1)
            .min(cursor.size - 1);
        let chunk = read(start, end).await?;
        if chunk.len() as u64 != end - start + 1 {
            return Err(ServerError::Internal("JSONL 附件 Range 数据不完整".into()));
        }
        if check_boundary && chunk.first() != Some(&b'\n') {
            return Err(ServerError::BadRequest(
                "JSONL 游标不在行边界，请重新打开预览",
            ));
        }
        first_chunk = false;
        for part in chunk[usize::from(check_boundary)..].split_inclusive(|byte| *byte == b'\n') {
            if line.len().saturating_add(part.len()) > MAX_LINE_BYTES + 2 {
                return Err(ServerError::BadRequest(LINE_TOO_LARGE));
            }
            line.extend_from_slice(part);
            cursor.offset += part.len() as u64;
            if part.last() == Some(&b'\n') || cursor.offset == cursor.size {
                let bytes = line.strip_suffix(b"\n").unwrap_or(&line);
                let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
                if bytes.len() > MAX_LINE_BYTES {
                    return Err(ServerError::BadRequest(LINE_TOO_LARGE));
                }
                // 极端长行时允许一页少于十行；未返回的行留给下一页，不丢弃或截断 JSON。
                if page_bytes + bytes.len() > MAX_PAGE_BYTES && !lines.is_empty() {
                    cursor.offset = line_start;
                    return finish_page(lines, cursor);
                }
                let bytes = if cursor.line_number == 1 {
                    bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)
                } else {
                    bytes
                };
                let (text, error) = match std::str::from_utf8(bytes) {
                    Ok(text) => (text.to_owned(), None),
                    Err(_) => (
                        String::from_utf8_lossy(bytes).into_owned(),
                        Some("该行不是有效的 UTF-8 文本，已显示替代字符"),
                    ),
                };
                lines.push(JsonlLine {
                    line_number: cursor.line_number,
                    text,
                    error,
                });
                page_bytes += bytes.len();
                cursor.line_number = cursor
                    .line_number
                    .checked_add(1)
                    .ok_or(ServerError::BadRequest("JSONL 行号超出有效范围"))?;
                line.clear();
                line_start = cursor.offset;
                // 满页即返回，尤其是单个 4 MiB 行：不能再下载下一整行才发现预算不足。
                if lines.len() == PAGE_LINES || page_bytes >= MAX_PAGE_BYTES {
                    return finish_page(lines, cursor);
                }
            }
        }
    }
    finish_page(lines, cursor)
}

fn finish_page(lines: Vec<JsonlLine>, cursor: Cursor) -> Result<JsonlPage, ServerError> {
    let next_cursor = if cursor.offset < cursor.size {
        Some(
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&cursor)
                    .map_err(|error| ServerError::Internal(error.to_string()))?,
            ),
        )
    } else {
        None
    };
    Ok(JsonlPage { lines, next_cursor })
}

#[cfg(test)]
#[path = "jsonl_preview_tests.rs"]
mod tests;
