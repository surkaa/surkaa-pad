use serde::Serialize;
use specta::Type;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::State;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::diaries::get_diary;
use crate::error::AppError;
use crate::state::AppState;
use crate::utils::message_sender::MessageSender;

pub(crate) mod entry;
pub(crate) mod entry_command;
pub(crate) mod preview_cache;
mod range_reader;

use range_reader::{DiaryAttachmentRangeSource, SeekableRangeReader};

const ZIP_LOCAL_FILE_MAGIC: &[u8] = b"PK\x03\x04";
const ZIP_EMPTY_ARCHIVE_MAGIC: &[u8] = b"PK\x05\x06";
const ZIP_SPANNED_ARCHIVE_MAGIC: &[u8] = b"PK\x07\x08";
const SEVEN_Z_MAGIC: &[u8] = b"7z\xBC\xAF\x27\x1C";

pub const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_ARCHIVE_DEPTH: usize = 64;
const MAX_ARCHIVE_PATH_BYTES: usize = 4_096;
const MAX_TOTAL_PATH_BYTES: usize = 8 * 1024 * 1024;
const ARCHIVE_PREVIEW_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveFormat {
    Zip,
    SevenZip,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePreviewEntry {
    pub path: String,
    pub is_directory: bool,
    #[specta(type = f64)]
    pub size: u64,
    #[specta(type = f64)]
    pub compressed_size: u64,
    pub modified_at: Option<String>,
    // ZIP 可以按条目标记；7z 的加密信息位于压缩块层级，无法可靠映射到每个条目。
    pub encrypted: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePreview {
    pub format: ArchiveFormat,
    #[specta(type = f64)]
    pub archive_size: u64,
    pub file_count: u32,
    pub directory_count: u32,
    #[specta(type = f64)]
    pub uncompressed_size: u64,
    pub is_solid: bool,
    pub encrypted: bool,
    pub entries: Vec<ArchivePreviewEntry>,
    pub revision: String,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum ArchivePreviewEvent {
    Started,
    Completed {
        preview: ArchivePreview,
    },
    PasswordRequired {
        #[specta(rename = "invalidPassword")]
        invalid_password: bool,
    },
    Cancelled,
    Error {
        message: String,
    },
}

#[derive(Debug, Error)]
pub enum ArchivePreviewError {
    #[error("不支持的压缩包格式，仅支持 ZIP 和 7z")]
    UnsupportedFormat,
    #[error("压缩包目录已加密，需要输入压缩包密码")]
    PasswordRequired,
    #[error("压缩包密码错误或文件已损坏")]
    InvalidPassword,
    #[error("压缩包内的条目超过 {limit} 个，无法安全预览")]
    TooManyEntries { limit: usize },
    #[error("压缩包条目路径不安全或超出限制：{0}")]
    InvalidEntryPath(String),
    #[error("压缩包目录信息过大，无法安全预览")]
    MetadataTooLarge,
    #[error("压缩包已损坏或格式不受支持：{0}")]
    InvalidArchive(String),
    #[error("读取压缩包失败：{0}")]
    Io(#[from] std::io::Error),
}

/// 解析附件中的 ZIP/7z 目录结构。任务支持通过 `cmd_cancel_task` 取消。
/// `password` 只用于压缩包自身的加密目录，不会写入日志或持久化。
/// # Arguments
/// * `event` - 接收开始、密码请求、完成或错误事件的通道
/// * `diary_id` - 附件所属日记 ID
/// * `attachment_id` - 附件 ID
/// * `password` - 可选的压缩包密码
/// # Returns
/// * `Result<String, AppError>` - 后台任务令牌，可通过 `cmd_cancel_task` 取消
#[tauri::command]
#[specta::specta]
pub fn cmd_preview_archive_attachment(
    state: State<'_, AppState>,
    event: Channel<ArchivePreviewEvent>,
    diary_id: String,
    attachment_id: String,
    password: Option<String>,
) -> Result<String, AppError> {
    let task_pool = state.task_pool();
    let state = state.inner().clone();
    let event: Arc<dyn MessageSender<ArchivePreviewEvent>> = Arc::new(event);
    Ok(task_pool.spawn_cancelable(move |cancellation| async move {
        let _ = event.send(ArchivePreviewEvent::Started);
        let result = run_archive_preview(
            state,
            diary_id,
            attachment_id,
            password.map(Zeroizing::new),
            cancellation.clone(),
        )
        .await;
        let terminal_event = match result {
            Ok(preview) => ArchivePreviewEvent::Completed { preview },
            Err(ArchivePreviewError::PasswordRequired) => ArchivePreviewEvent::PasswordRequired {
                invalid_password: false,
            },
            Err(ArchivePreviewError::InvalidPassword) => ArchivePreviewEvent::PasswordRequired {
                invalid_password: true,
            },
            Err(_error) if cancellation.is_cancelled() => ArchivePreviewEvent::Cancelled,
            Err(error) => ArchivePreviewEvent::Error {
                message: error.to_string(),
            },
        };
        let _ = event.send(terminal_event);
    }))
}

async fn run_archive_preview(
    state: AppState,
    diary_id: String,
    attachment_id: String,
    password: Option<Zeroizing<String>>,
    cancellation: CancellationToken,
) -> Result<ArchivePreview, ArchivePreviewError> {
    let _storage_guard = state
        .lock_storage_operation()
        .await
        .map_err(|e| ArchivePreviewError::Io(std::io::Error::other(e)))?;
    if cancellation.is_cancelled() {
        return Err(cancelled_io().into());
    }

    let store = state.diary_store();
    let diary = get_diary(&state.diary_cache(), &state.crypto(), &*store, &diary_id)
        .await
        .map_err(|error| ArchivePreviewError::InvalidArchive(error.to_string()))?;
    let attachment = diary
        .attachments
        .into_iter()
        .find(|attachment| attachment.id == attachment_id)
        .ok_or_else(|| {
            ArchivePreviewError::InvalidArchive(format!("附件不存在：{attachment_id}"))
        })?;
    let revision = entry_command::source_revision(&state, &diary_id, &attachment_id).await?;
    let verification_state = state.clone();
    let verification_id = diary_id.clone();
    let verification_attachment_id = attachment_id.clone();
    let mut attachment = attachment;
    attachment.etag = Some(revision.clone());
    let archive_size = store
        .get_attachment_size(&diary_id, &attachment_id, attachment.etag.as_deref())
        .await
        .map_err(|error| ArchivePreviewError::Io(std::io::Error::other(error.to_string())))?;
    let runtime = tokio::runtime::Handle::current();
    let parse_cancellation = CancellationToken::new();
    let source_cancellation = parse_cancellation.clone();
    let mut parse_task = tokio::task::spawn_blocking(move || {
        let source = DiaryAttachmentRangeSource::new(
            runtime,
            store,
            state.crypto(),
            diary_id,
            attachment,
            source_cancellation,
        );
        let reader = SeekableRangeReader::new(source, archive_size);
        preview_archive(
            reader,
            archive_size,
            password.as_ref().map(|value| value.as_str()),
        )
    });

    let result = tokio::select! {
        _ = cancellation.cancelled() => {
            parse_cancellation.cancel();
            // spawn_blocking 无法强制中止；等待 Range 读取响应取消，避免任务脱离存储操作锁继续访问对象。
            let _ = parse_task.await;
            Err(cancelled_io().into())
        },
        result = tokio::time::timeout(ARCHIVE_PREVIEW_TIMEOUT, &mut parse_task) => {
            match result {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => Err(ArchivePreviewError::InvalidArchive(format!("解析任务异常结束：{error}"))),
                Err(_) => {
                    parse_cancellation.cancel();
                    let _ = parse_task.await;
                    Err(ArchivePreviewError::InvalidArchive("读取压缩包目录超时".into()))
                }
            }
        }
    };
    let mut preview = result?;
    if revision
        != entry_command::source_revision(
            &verification_state,
            &verification_id,
            &verification_attachment_id,
        )
        .await?
    {
        return Err(ArchivePreviewError::InvalidArchive(
            "压缩包已变化，请重新打开目录".into(),
        ));
    }
    preview.revision = revision;
    Ok(preview)
}

fn cancelled_io() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Interrupted, "压缩包预览已取消")
}

pub fn preview_archive<R: Read + Seek>(
    mut reader: R,
    archive_size: u64,
    password: Option<&str>,
) -> Result<ArchivePreview, ArchivePreviewError> {
    let format = detect_format(&mut reader)?;
    reader.seek(SeekFrom::Start(0))?;
    match format {
        ArchiveFormat::Zip => preview_zip(reader, archive_size),
        ArchiveFormat::SevenZip => preview_seven_zip(reader, archive_size, password),
    }
}

fn detect_format<R: Read + Seek>(reader: &mut R) -> Result<ArchiveFormat, ArchivePreviewError> {
    let mut signature = [0_u8; 6];
    let read = reader.read(&mut signature)?;
    reader.seek(SeekFrom::Start(0))?;
    if read >= 4
        && [
            &ZIP_LOCAL_FILE_MAGIC,
            &ZIP_EMPTY_ARCHIVE_MAGIC,
            &ZIP_SPANNED_ARCHIVE_MAGIC,
        ]
        .iter()
        .any(|magic| signature.starts_with(magic))
    {
        return Ok(ArchiveFormat::Zip);
    }
    if read >= SEVEN_Z_MAGIC.len() && signature.starts_with(SEVEN_Z_MAGIC) {
        return Ok(ArchiveFormat::SevenZip);
    }
    Err(ArchivePreviewError::UnsupportedFormat)
}

fn preview_zip<R: Read + Seek>(
    reader: R,
    archive_size: u64,
) -> Result<ArchivePreview, ArchivePreviewError> {
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|error| ArchivePreviewError::InvalidArchive(error.to_string()))?;
    ensure_entry_count(archive.len())?;

    let mut entries = Vec::with_capacity(archive.len());
    let mut limits = EntryLimits::default();
    let mut encrypted = false;
    for index in 0..archive.len() {
        // raw 模式只定位本地文件头以取得元数据，不读取或解压文件内容。
        let file = archive
            .by_index_raw(index)
            .map_err(|error| ArchivePreviewError::InvalidArchive(error.to_string()))?;
        let path = limits.validate_path(file.name())?;
        encrypted |= file.encrypted();
        entries.push(ArchivePreviewEntry {
            path,
            is_directory: file.is_dir(),
            size: file.size(),
            compressed_size: file.compressed_size(),
            modified_at: file.last_modified().map(format_zip_datetime),
            encrypted: Some(file.encrypted()),
        });
    }
    build_preview(ArchiveFormat::Zip, archive_size, false, encrypted, entries)
}

fn preview_seven_zip<R: Read + Seek>(
    mut reader: R,
    archive_size: u64,
    password: Option<&str>,
) -> Result<ArchivePreview, ArchivePreviewError> {
    let password_value = password.unwrap_or_default();
    let password = sevenz_rust2::Password::new(password_value);
    let archive = entry::read_bounded_seven_zip(&mut reader, &password)?;
    ensure_entry_count(archive.files.len())?;

    let encrypted = archive.blocks.iter().any(|block| {
        block
            .coders
            .iter()
            .any(|coder| coder.encoder_method_id() == sevenz_rust2::EncoderMethod::ID_AES256_SHA256)
    });
    let mut entries = Vec::with_capacity(archive.files.len());
    let mut limits = EntryLimits::default();
    for file in &archive.files {
        let path = limits.validate_path(file.name())?;
        let modified_at = file.has_last_modified_date.then(|| {
            let system_time: std::time::SystemTime = file.last_modified_date.into();
            chrono::DateTime::<chrono::Local>::from(system_time)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        });
        entries.push(ArchivePreviewEntry {
            path,
            is_directory: file.is_directory(),
            size: file.size,
            compressed_size: file.compressed_size,
            modified_at,
            encrypted: None,
        });
    }
    build_preview(
        ArchiveFormat::SevenZip,
        archive_size,
        archive.is_solid,
        encrypted,
        entries,
    )
}

fn build_preview(
    format: ArchiveFormat,
    archive_size: u64,
    is_solid: bool,
    encrypted: bool,
    entries: Vec<ArchivePreviewEntry>,
) -> Result<ArchivePreview, ArchivePreviewError> {
    let file_count = entries.iter().filter(|entry| !entry.is_directory).count();
    let directory_count = entries.len().saturating_sub(file_count);
    let uncompressed_size = entries
        .iter()
        .filter(|entry| !entry.is_directory)
        .try_fold(0_u64, |total, entry| total.checked_add(entry.size))
        .ok_or(ArchivePreviewError::MetadataTooLarge)?;
    Ok(ArchivePreview {
        format,
        archive_size,
        file_count: u32::try_from(file_count).unwrap_or(u32::MAX),
        directory_count: u32::try_from(directory_count).unwrap_or(u32::MAX),
        uncompressed_size,
        is_solid,
        encrypted,
        entries,
        revision: String::new(),
    })
}

fn ensure_entry_count(count: usize) -> Result<(), ArchivePreviewError> {
    if count > MAX_ARCHIVE_ENTRIES {
        Err(ArchivePreviewError::TooManyEntries {
            limit: MAX_ARCHIVE_ENTRIES,
        })
    } else {
        Ok(())
    }
}

#[derive(Default)]
struct EntryLimits {
    total_path_bytes: usize,
}

impl EntryLimits {
    fn validate_path(&mut self, raw_path: &str) -> Result<String, ArchivePreviewError> {
        let path = raw_path.replace('\\', "/");
        let display_path = path.trim_end_matches('/');
        let path_bytes = display_path.len();
        let mut components = display_path.split('/');
        let first = components.next().unwrap_or_default();
        let remaining: Vec<&str> = components.collect();
        let depth = usize::from(!first.is_empty()) + remaining.len();
        let has_unsafe_component = std::iter::once(first)
            .chain(remaining.iter().copied())
            .any(|component| component.is_empty() || component == "." || component == "..");
        let drive_prefixed = first.as_bytes().get(1) == Some(&b':');
        if display_path.is_empty()
            || path.starts_with('/')
            || drive_prefixed
            || has_unsafe_component
            || depth > MAX_ARCHIVE_DEPTH
            || path_bytes > MAX_ARCHIVE_PATH_BYTES
        {
            return Err(ArchivePreviewError::InvalidEntryPath(truncate_for_error(
                display_path,
            )));
        }
        self.total_path_bytes = self
            .total_path_bytes
            .checked_add(path_bytes)
            .ok_or(ArchivePreviewError::MetadataTooLarge)?;
        if self.total_path_bytes > MAX_TOTAL_PATH_BYTES {
            return Err(ArchivePreviewError::MetadataTooLarge);
        }
        Ok(display_path.to_string())
    }
}

fn truncate_for_error(value: &str) -> String {
    let mut chars = value.chars();
    let prefix: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn format_zip_datetime(value: zip::DateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        value.year(),
        value.month(),
        value.day(),
        value.hour(),
        value.minute(),
        value.second()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    const PLAIN_SEVEN_Z: &str = "N3q8ryccAAPFVQ6hkQAAAAAAAAAhAAAAAAAAALz3Ey0AMxpJ1reRjN5PbnHnTQdyp1DzcTM5VWztEUaO/w5KQAAAAIEzB64P0B3SfJ8/R0FnN9Z25l7u6gTp5NJ7TyE+pM4A6htaK47zOKPmYkViCGBcQwNXjP7cf2WdZBzYc2CMoFSB5uiNUIYNIvzQ+C53Dy4AaVH8hLAUbgaH3jl1jNP2kOo9GTmknt6h//+DRwAAFwYhAQlwAAcLAQABIwMBAQVdAACAAAyAogoBfV1qRgAA";
    const ENCRYPTED_SEVEN_Z: &str = "N3q8ryccAATTIEOsgAAAAAAAAAAuAAAAAAAAAGT7LqbyWJG2NLKlQ9TUF7X2jeW2xmC6bo9eP2YPeeyhWKL/0RrS+DWWjNwZPhIpEzw181gfE19d0ivb+cTXE8jn4dEaGGBl731cukMmVSdpD1b4Kr0FSbAMaukuXH2LrVUOfYHvcasNhpJEw3Sp83TxKfTeyUJo4ymzgZbZth4bqo9BbBcGEAEJcAAHCwEAASQG8QcBElMPdznbYE8TDQsVayWhjxXfMQxiCgFwuLliAAA=";

    fn extract_test(
        bytes: &[u8],
        index: usize,
        path: &str,
        size: u64,
        password: Option<&str>,
    ) -> Result<(Vec<u8>, Vec<entry::ArchiveEntryProgress>), ArchivePreviewError> {
        let progress = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = progress.clone();
        let mut control = entry::EntryControl::new(
            CancellationToken::new(),
            std::sync::Arc::new(move |value| sink.lock().unwrap().push(value)),
        );
        let mut output = Vec::new();
        entry::extract_entry(
            Cursor::new(bytes),
            &entry::EntryRequest {
                index,
                path,
                size,
                password,
            },
            &mut output,
            &mut control,
        )?;
        let events = progress.lock().unwrap().clone();
        Ok((output, events))
    }

    #[test]
    fn extracts_only_requested_zip_entry_and_reports_byte_progress() {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("first.txt", options).unwrap();
        writer.write_all(b"not requested").unwrap();
        let expected = vec![b'A'; 3 * 1024 * 1024];
        writer.start_file("second.txt", options).unwrap();
        writer.write_all(&expected).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let (actual, progress) =
            extract_test(&bytes, 1, "second.txt", expected.len() as u64, None).unwrap();
        assert_eq!(actual, expected);
        assert!(progress.len() > 10 && progress.len() <= 102);
        assert_eq!(
            progress.last().unwrap().processed_bytes,
            expected.len() as u64
        );
        assert_eq!(progress.last().unwrap().total_bytes, expected.len() as u64);
        assert!(progress
            .windows(2)
            .all(|p| p[0].processed_bytes <= p[1].processed_bytes));
    }

    #[test]
    fn extracts_aes_zip_and_requires_the_correct_password() {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "secret.json",
                SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "password"),
            )
            .unwrap();
        writer.write_all(b"{\"secret\":true}").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(matches!(
            extract_test(&bytes, 0, "secret.json", 15, None),
            Err(ArchivePreviewError::PasswordRequired)
        ));
        assert!(matches!(
            extract_test(&bytes, 0, "secret.json", 15, Some("wrong")),
            Err(ArchivePreviewError::InvalidPassword)
        ));
        assert_eq!(
            extract_test(&bytes, 0, "secret.json", 15, Some("password"))
                .unwrap()
                .0,
            b"{\"secret\":true}"
        );
    }

    #[test]
    fn refuses_changed_entries_traversal_links_nested_archives_and_wrong_sizes() {
        let bytes = zip_bytes();
        for (index, path, size) in [
            (1, "other.txt", 5),
            (1, "资料/readme.txt", 6),
            (0, "资料/", 0),
            (999, "missing", 0),
        ] {
            assert!(extract_test(&bytes, index, path, size, None).is_err());
        }
        for path in ["../evil.txt", "/root.txt", "C:/evil.txt", "nested.zip"] {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            writer
                .start_file(path, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"12345").unwrap();
            let bytes = writer.finish().unwrap().into_inner();
            assert!(extract_test(&bytes, 0, path, 5, None).is_err(), "{path}");
        }
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .add_symlink("link.txt", "outside", SimpleFileOptions::default())
            .unwrap();
        assert!(extract_test(
            &writer.finish().unwrap().into_inner(),
            0,
            "link.txt",
            7,
            None
        )
        .is_err());
    }

    #[test]
    fn rejects_corrupt_zip_instead_of_publishing_partial_data() {
        let mut bytes = zip_bytes();
        let pos = bytes
            .windows(5)
            .position(|value| value == b"hello")
            .unwrap();
        bytes[pos] ^= 1;
        assert!(extract_test(&bytes, 1, "资料/readme.txt", 5, None).is_err());
    }

    #[test]
    fn extracts_every_seven_zip_entry_including_solid_blocks() {
        for (fixture, password) in [(PLAIN_SEVEN_Z, None), (ENCRYPTED_SEVEN_Z, Some("correct"))] {
            let bytes = decode_fixture(fixture);
            let preview =
                preview_archive(Cursor::new(&bytes), bytes.len() as u64, password).unwrap();
            for (index, item) in preview
                .entries
                .iter()
                .enumerate()
                .filter(|(_, item)| !item.is_directory)
            {
                let (output, progress) =
                    extract_test(&bytes, index, &item.path, item.size, password).unwrap();
                assert_eq!(output.len() as u64, item.size);
                if let Some(last) = progress.last() {
                    assert_eq!(last.processed_bytes, last.total_bytes);
                }
            }
        }
    }

    #[test]
    fn cancellation_and_declared_limits_fail_before_writing_output() {
        let token = CancellationToken::new();
        token.cancel();
        let mut control = entry::EntryControl::new(token, std::sync::Arc::new(|_| {}));
        let mut output = Vec::new();
        assert!(entry::extract_entry(
            Cursor::new(zip_bytes()),
            &entry::EntryRequest {
                index: 1,
                path: "资料/readme.txt",
                size: 5,
                password: None
            },
            &mut output,
            &mut control
        )
        .is_err());
        assert!(output.is_empty());
        assert!(extract_test(
            &zip_bytes(),
            1,
            "资料/readme.txt",
            entry::MAX_ENTRY_BYTES + 1,
            None
        )
        .is_err());
    }

    fn zip_bytes() -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut output);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.add_directory("资料/", options).unwrap();
            writer.start_file("资料/readme.txt", options).unwrap();
            writer.write_all(b"hello").unwrap();
            writer.finish().unwrap();
        }
        output.into_inner()
    }

    fn decode_fixture(value: &str) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(value)
            .unwrap()
    }

    #[test]
    fn previews_zip_metadata_without_extracting_entries() {
        let bytes = zip_bytes();
        let preview = preview_archive(Cursor::new(&bytes), bytes.len() as u64, None).unwrap();

        assert_eq!(preview.format, ArchiveFormat::Zip);
        assert_eq!(preview.file_count, 1);
        assert_eq!(preview.directory_count, 1);
        assert_eq!(preview.uncompressed_size, 5);
        assert_eq!(preview.entries[1].path, "资料/readme.txt");
        assert_eq!(preview.entries[1].encrypted, Some(false));
    }

    #[test]
    fn previews_seven_zip_metadata() {
        let bytes = decode_fixture(PLAIN_SEVEN_Z);
        let preview = preview_archive(Cursor::new(&bytes), bytes.len() as u64, None).unwrap();

        assert_eq!(preview.format, ArchiveFormat::SevenZip);
        assert_eq!(preview.file_count, 2);
        assert_eq!(preview.directory_count, 0);
        assert!(preview.uncompressed_size > 0);
    }

    #[test]
    fn encrypted_seven_zip_header_requests_and_validates_password() {
        let bytes = decode_fixture(ENCRYPTED_SEVEN_Z);
        assert!(matches!(
            preview_archive(Cursor::new(&bytes), bytes.len() as u64, None),
            Err(ArchivePreviewError::PasswordRequired)
        ));
        assert!(matches!(
            preview_archive(Cursor::new(&bytes), bytes.len() as u64, Some("wrong")),
            Err(ArchivePreviewError::InvalidPassword)
        ));
        let preview =
            preview_archive(Cursor::new(&bytes), bytes.len() as u64, Some("correct")).unwrap();
        assert!(preview.encrypted);
    }

    #[test]
    fn rejects_unknown_format_and_unsafe_paths() {
        assert!(matches!(
            preview_archive(Cursor::new(b"not archive"), 11, None),
            Err(ArchivePreviewError::UnsupportedFormat)
        ));
        let mut limits = EntryLimits::default();
        for path in ["../secret", "/absolute", "C:/drive", "folder//file"] {
            assert!(matches!(
                limits.validate_path(path),
                Err(ArchivePreviewError::InvalidEntryPath(_))
            ));
        }
    }
}
