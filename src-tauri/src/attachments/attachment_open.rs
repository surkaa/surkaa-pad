use crate::android_attachment_opener::open_external_file;
use crate::attachments::attachment_types::AttachmentProcessEvent;
use crate::attachments::{AttachmentError, AttachmentMeta};
use crate::diaries::get_diary;
use crate::state::AppState;
use crate::stream::ByteStream;
use crate::utils::id_generate::generate_descending_id;
use crate::utils::message_sender::MessageSender;
use futures_util::StreamExt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

const EXTERNAL_OPEN_DIRECTORY: &str = "open-attachments";
const EXTERNAL_OPEN_FILE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_TEMP_FILENAME_BYTES: usize = 180;

pub async fn open_html_attachment(
    app: &AppHandle,
    state: &AppState,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    app_cache_dir: &Path,
    diary_id: &str,
    attachment_id: &str,
    cancellation: CancellationToken,
) {
    let _ = event.send(AttachmentProcessEvent::Started);
    let result = prepare_html_attachment(
        state,
        event.clone(),
        app_cache_dir,
        diary_id,
        attachment_id,
        &cancellation,
    )
    .await
    .and_then(|mut prepared| {
        if cancellation.is_cancelled() {
            return Err(AttachmentError::InvalidOperation("打开附件已取消".into()));
        }
        let _ = event.send(AttachmentProcessEvent::Finalizing);
        open_external_file(app, &prepared.path, "text/html", &prepared.display_name)
            .map_err(|error| AttachmentError::InvalidOperation(error.message))?;
        prepared.keep();
        Ok(())
    });

    match result {
        Ok(()) => {
            let _ = event.send(AttachmentProcessEvent::CompletedWithoutData);
        }
        Err(error) => {
            let _ = event.send(AttachmentProcessEvent::Error(error.to_string()));
        }
    }
}

async fn prepare_html_attachment(
    state: &AppState,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    app_cache_dir: &Path,
    diary_id: &str,
    attachment_id: &str,
    cancellation: &CancellationToken,
) -> Result<PreparedExternalAttachment, AttachmentError> {
    if diary_id.trim().is_empty() || attachment_id.trim().is_empty() {
        return Err(AttachmentError::InvalidOperation(
            "日记 ID 和附件 ID 不能为空".into(),
        ));
    }

    let store = state.diary_store();
    let diary = get_diary(&state.diary_cache(), &state.crypto(), &*store, diary_id).await?;
    let attachment = diary
        .attachments
        .into_iter()
        .find(|attachment| attachment.id == attachment_id)
        .ok_or_else(|| AttachmentError::InvalidOperation("附件不存在".into()))?;
    if !is_html_attachment(&attachment) {
        return Err(AttachmentError::InvalidOperation(
            "当前只支持使用外部应用打开 HTML 附件".into(),
        ));
    }
    if cancellation.is_cancelled() {
        return Err(AttachmentError::InvalidOperation("打开附件已取消".into()));
    }

    let stream = store
        .download_attachment(diary_id, attachment_id, None, attachment.etag.as_deref())
        .await?;
    let stream = if attachment.encrypted {
        state
            .crypto()
            .decrypt_streaming(stream, &attachment.nonce, 0)?
    } else {
        stream
    };

    write_external_open_file(
        app_cache_dir,
        &attachment.filename,
        attachment_id,
        attachment.size,
        stream,
        event,
        cancellation,
    )
    .await
}

async fn write_external_open_file(
    app_cache_dir: &Path,
    filename: &str,
    attachment_id: &str,
    expected_size: u64,
    mut stream: ByteStream,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    cancellation: &CancellationToken,
) -> Result<PreparedExternalAttachment, AttachmentError> {
    let display_name = sanitize_html_filename(filename, attachment_id);
    let directory = app_cache_dir
        .join(EXTERNAL_OPEN_DIRECTORY)
        .join(generate_descending_id());
    tokio::fs::create_dir_all(&directory).await?;
    let path = directory.join(&display_name);
    let partial_path = directory.join(format!("{display_name}.part"));
    let mut guard = PartialExternalAttachment::new(directory, partial_path.clone());
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&partial_path)
        .await?;
    let mut written = 0_u64;

    loop {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => {
                return Err(AttachmentError::InvalidOperation("打开附件已取消".into()));
            }
            chunk = stream.next() => chunk,
        };
        let Some(chunk) = chunk else { break };
        let chunk = chunk.map_err(|error| {
            AttachmentError::FileOperationFailed(format!("读取附件失败：{error}"))
        })?;
        file.write_all(&chunk).await?;
        written = written.saturating_add(chunk.len() as u64);
        let progress = if expected_size == 0 {
            0
        } else {
            (written as u128 * 99 / expected_size as u128).min(99) as u8
        };
        let _ = event.send(AttachmentProcessEvent::Progress(progress));
    }

    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(&partial_path, &path).await?;
    guard.partial_path = None;

    Ok(PreparedExternalAttachment {
        path,
        display_name,
        directory: guard.directory.take(),
    })
}

struct PartialExternalAttachment {
    directory: Option<PathBuf>,
    partial_path: Option<PathBuf>,
}

impl PartialExternalAttachment {
    fn new(directory: PathBuf, partial_path: PathBuf) -> Self {
        Self {
            directory: Some(directory),
            partial_path: Some(partial_path),
        }
    }
}

impl Drop for PartialExternalAttachment {
    fn drop(&mut self) {
        if let Some(path) = self.partial_path.take() {
            let _ = fs::remove_file(path);
        }
        if let Some(directory) = self.directory.take() {
            let _ = fs::remove_dir_all(directory);
        }
    }
}

#[derive(Debug)]
struct PreparedExternalAttachment {
    path: PathBuf,
    display_name: String,
    directory: Option<PathBuf>,
}

impl PreparedExternalAttachment {
    fn keep(&mut self) {
        self.directory = None;
    }
}

impl Drop for PreparedExternalAttachment {
    fn drop(&mut self) {
        if let Some(directory) = self.directory.take() {
            let _ = fs::remove_dir_all(directory);
        }
    }
}

fn is_html_attachment(attachment: &AttachmentMeta) -> bool {
    let mimetype = attachment
        .mimetype
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if matches!(mimetype.as_str(), "text/html" | "application/xhtml+xml") {
        return true;
    }
    let filename = attachment.filename.to_ascii_lowercase();
    filename.ends_with(".html") || filename.ends_with(".htm") || filename.ends_with(".xhtml")
}

fn sanitize_html_filename(filename: &str, attachment_id: &str) -> String {
    let mut sanitized = filename
        .trim()
        .chars()
        .map(|character| match character {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    sanitized = sanitized.trim_matches([' ', '.']).to_string();
    if sanitized.is_empty() || matches!(sanitized.as_str(), "." | "..") {
        sanitized = attachment_id.to_string();
    }
    sanitized = truncate_utf8(&sanitized, MAX_TEMP_FILENAME_BYTES);
    let lowercase = sanitized.to_ascii_lowercase();
    if !lowercase.ends_with(".html")
        && !lowercase.ends_with(".htm")
        && !lowercase.ends_with(".xhtml")
    {
        sanitized.push_str(".html");
    }
    sanitized
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

pub fn cleanup_stale_external_open_files(app_cache_dir: &Path) -> std::io::Result<()> {
    let root = app_cache_dir.join(EXTERNAL_OPEN_DIRECTORY);
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let now = SystemTime::now();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= EXTERNAL_OPEN_FILE_TTL);
        let contains_partial = metadata.is_dir()
            && fs::read_dir(&path)
                .map(|children| {
                    children.filter_map(Result::ok).any(|child| {
                        child
                            .file_name()
                            .to_string_lossy()
                            .to_ascii_lowercase()
                            .ends_with(".part")
                    })
                })
                .unwrap_or(false);
        if stale || contains_partial {
            if metadata.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachments::attachment::add_attachment_with_result;
    use crate::caches::LocalObjectStore;
    use crate::cryptos::Crypto;
    use crate::diaries::save_diary;
    use crate::object::OssClient;
    use crate::stream::create_mock_stream;
    use bytes::Bytes;
    use futures_util::stream;
    use std::io;
    use tokio::sync::mpsc;

    fn attachment(filename: &str, mimetype: &str) -> AttachmentMeta {
        AttachmentMeta {
            id: "att-html".into(),
            filename: filename.into(),
            mimetype: mimetype.into(),
            size: 0,
            encrypted: false,
            nonce: Vec::new(),
            algorithm: crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm,
            etag: None,
            content_info: None,
        }
    }

    #[test]
    fn recognizes_html_by_mimetype_or_filename() {
        assert!(is_html_attachment(&attachment(
            "page.bin",
            "text/html; charset=utf-8"
        )));
        assert!(is_html_attachment(&attachment(
            "page.XHTML",
            "application/octet-stream"
        )));
        assert!(!is_html_attachment(&attachment("page.txt", "text/plain")));
    }

    #[test]
    fn sanitizes_untrusted_filenames_and_preserves_html_extension() {
        assert_eq!(
            sanitize_html_filename("../../bad:name?.html", "att-1"),
            "_.._bad_name_.html"
        );
        assert_eq!(sanitize_html_filename("   ", "att-1"), "att-1.html");
        let long = format!("{}.html", "界".repeat(100));
        let safe = sanitize_html_filename(&long, "att-1");
        assert!(safe.len() <= MAX_TEMP_FILENAME_BYTES + ".html".len());
        assert!(safe.is_char_boundary(safe.len()));
    }

    #[tokio::test]
    async fn removes_partial_plaintext_when_preparation_fails() {
        let temp = tempfile::tempdir().unwrap();
        let stream: ByteStream = Box::pin(stream::iter([
            Ok(Bytes::from_static(b"<html>")),
            Err(io::Error::other("simulated read failure")),
        ]));
        let (sender, _receiver) = mpsc::unbounded_channel();
        let result = write_external_open_file(
            temp.path(),
            "page.html",
            "att-1",
            12,
            stream,
            Arc::new(sender),
            &CancellationToken::new(),
        )
        .await;

        assert!(result.is_err());
        let root = temp.path().join(EXTERNAL_OPEN_DIRECTORY);
        assert!(root.read_dir().unwrap().next().is_none());
    }

    #[tokio::test]
    async fn removes_partial_plaintext_when_preparation_is_canceled() {
        let temp = tempfile::tempdir().unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let (sender, _receiver) = mpsc::unbounded_channel();
        let result = write_external_open_file(
            temp.path(),
            "page.html",
            "att-1",
            6,
            create_mock_stream(b"secret".to_vec(), 6),
            Arc::new(sender),
            &cancellation,
        )
        .await;

        assert!(result.unwrap_err().to_string().contains("已取消"));
        let root = temp.path().join(EXTERNAL_OPEN_DIRECTORY);
        assert!(root.read_dir().unwrap().next().is_none());
    }

    #[test]
    fn startup_cleanup_removes_crash_fragments_but_keeps_recent_completed_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(EXTERNAL_OPEN_DIRECTORY);
        let partial_dir = root.join("partial");
        let completed_dir = root.join("completed");
        fs::create_dir_all(&partial_dir).unwrap();
        fs::create_dir_all(&completed_dir).unwrap();
        fs::write(partial_dir.join("page.html.part"), b"partial plaintext").unwrap();
        fs::write(completed_dir.join("page.html"), b"complete plaintext").unwrap();

        cleanup_stale_external_open_files(temp.path()).unwrap();

        assert!(!partial_dir.exists());
        assert!(completed_dir.join("page.html").exists());
    }

    #[tokio::test]
    async fn prepares_plain_and_encrypted_html_with_exact_contents() {
        for encrypted in [false, true] {
            let storage = tempfile::tempdir().unwrap();
            let cache = tempfile::tempdir().unwrap();
            let crypto = Crypto::new();
            crypto
                .derive_dek("password".into(), "YXR0YWNobWVudC1vcGVuLXRlc3Qtc2FsdA")
                .unwrap();
            let state = AppState::from_parts(
                crypto.clone(),
                OssClient::new(),
                LocalObjectStore::new(storage.path().to_path_buf()),
            );
            let (summary, _) = save_diary(
                &state.diary_cache(),
                &crypto,
                &*state.diary_store(),
                "html test",
            )
            .await
            .unwrap();
            let body = b"<!doctype html><script>globalThis.answer = 42</script>".to_vec();
            let (sender, _receiver) = mpsc::unbounded_channel();
            let (meta, _) = add_attachment_with_result(
                &state,
                Arc::new(sender),
                &summary.id,
                encrypted,
                body.len() as u64,
                "text/html".into(),
                create_mock_stream(body.clone(), body.len()),
                Some("test page.html".into()),
            )
            .await
            .unwrap();
            let (sender, _receiver) = mpsc::unbounded_channel();
            let prepared = prepare_html_attachment(
                &state,
                Arc::new(sender),
                cache.path(),
                &summary.id,
                &meta.id,
                &CancellationToken::new(),
            )
            .await
            .unwrap();

            assert_eq!(tokio::fs::read(&prepared.path).await.unwrap(), body);
            assert_eq!(prepared.display_name, "test page.html");
            assert!(!prepared.path.with_file_name("test page.html.part").exists());
        }
    }
}
