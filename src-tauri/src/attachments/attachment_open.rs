use crate::attachments::{AttachmentError, AttachmentMeta};
use crate::diaries::get_diary;
use crate::state::AppState;
use std::fs;
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

const LEGACY_EXTERNAL_OPEN_DIRECTORY: &str = "open-attachments";

pub async fn open_html_attachment(
    app: &AppHandle,
    state: &AppState,
    diary_id: &str,
    attachment_id: &str,
) -> Result<(), AttachmentError> {
    if diary_id.trim().is_empty() || attachment_id.trim().is_empty() {
        return Err(AttachmentError::InvalidOperation(
            "日记 ID 和附件 ID 不能为空".into(),
        ));
    }

    let store = state.diary_store();
    let diary = get_diary(&state.diary_cache(), &state.crypto(), &*store, diary_id).await?;
    let attachment = diary
        .attachments
        .iter()
        .find(|attachment| attachment.id == attachment_id)
        .ok_or_else(|| AttachmentError::InvalidOperation("附件不存在".into()))?;
    if !is_html_attachment(attachment) {
        return Err(AttachmentError::InvalidOperation(
            "当前只支持使用外部浏览器打开 HTML 附件".into(),
        ));
    }

    let url = state.attachment_server().html_url(diary_id, attachment_id);
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| AttachmentError::InvalidOperation(error.to_string()))
}

pub(super) fn is_html_attachment(attachment: &AttachmentMeta) -> bool {
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

pub fn cleanup_legacy_html_temp_files(app_cache_dir: &Path) -> std::io::Result<()> {
    let path = app_cache_dir.join(LEGACY_EXTERNAL_OPEN_DIRECTORY);
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn removes_legacy_plaintext_temp_directory_idempotently() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join(LEGACY_EXTERNAL_OPEN_DIRECTORY);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("page.html"), b"plaintext").unwrap();

        cleanup_legacy_html_temp_files(temp.path()).unwrap();
        cleanup_legacy_html_temp_files(temp.path()).unwrap();

        assert!(!legacy.exists());
    }
}
