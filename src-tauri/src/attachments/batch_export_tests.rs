use super::*;
use crate::cryptos::{crypto_types::EncryptionAlgorithm, Crypto};
use crate::stream::create_mock_stream;
use std::io::Cursor;

fn attachment(id: &str) -> AttachmentMeta {
    AttachmentMeta {
        id: id.into(),
        filename: "文件.txt".into(),
        mimetype: "text/plain".into(),
        size: 10,
        encrypted: false,
        nonce: vec![],
        algorithm: EncryptionAlgorithm::Ctr,
        etag: None,
        content_info: None,
    }
}

#[test]
fn selection_is_validated_and_deduplicated_before_export() {
    let attachments = vec![attachment("a"), attachment("b")];
    assert!(selected_attachments(&attachments, &[]).is_err());
    assert!(selected_attachments(&attachments, &["missing".into()]).is_err());
    assert_eq!(
        selected_attachments(&attachments, &["b".into(), "a".into(), "b".into()])
            .unwrap()
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["b", "a"]
    );
}

#[test]
fn filenames_cannot_escape_the_chosen_directory_or_open_windows_devices() {
    for source in [
        "../a.txt",
        "..\\a.txt",
        "/etc/passwd",
        "C:\\a.txt",
        "bad\0name",
    ] {
        let safe = safe_filename(source);
        assert_eq!(Path::new(&safe).components().count(), 1);
        assert!(!safe
            .chars()
            .any(|c| c == '/' || c == '\\' || c == ':' || c.is_control()));
    }
    for source in [
        "CON",
        "nul.txt",
        "NUL .txt",
        "COM1.mp4",
        "COM¹.txt",
        "LPT9",
        "CONOUT$",
    ] {
        assert!(safe_filename(source).starts_with('_'), "{source}");
    }
    assert_eq!(safe_filename("..."), "attachment");
    assert_eq!(safe_filename("   "), "attachment");
    assert_eq!(safe_filename("报告 最终版.pdf"), "报告 最终版.pdf");
    let long = safe_filename(&format!("{}.pdf", "测试😀".repeat(100)));
    assert!(long.len() <= 180);
    assert!(long.ends_with(".pdf"));
}

#[test]
fn numbering_keeps_extensions_and_reserves_without_overwriting_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("test.txt"), b"original").unwrap();
    std::fs::write(directory.path().join("test (1).txt"), b"previous").unwrap();
    let (file, path) = create_local_file(directory.path(), "test.txt").unwrap();
    assert_eq!(path.file_name().unwrap(), "test (2).txt");
    drop(file);
    assert_eq!(
        std::fs::read(directory.path().join("test.txt")).unwrap(),
        b"original"
    );
    assert_eq!(
        std::fs::read(directory.path().join("test (1).txt")).unwrap(),
        b"previous"
    );
    assert_eq!(numbered_filename(".gitignore", 1), ".gitignore (1)");
    assert_eq!(numbered_filename("无扩展名", 1), "无扩展名 (1)");
}

#[test]
fn concurrent_exports_reserve_different_destinations() {
    let directory = tempfile::tempdir().unwrap();
    let handles: Vec<_> = (0..20)
        .map(|_| {
            let path = directory.path().to_owned();
            std::thread::spawn(move || create_local_file(&path, "same.dat").unwrap().1)
        })
        .collect();
    let names: HashSet<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(names.len(), 20);
}

#[test]
fn a_directory_with_the_same_name_is_not_overwritten_or_reported_as_a_file_error() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("test.txt")).unwrap();
    let (file, path) = create_local_file(directory.path(), "test.txt").unwrap();
    drop(file);
    assert_eq!(path.file_name().unwrap(), "test (1).txt");
    assert!(directory.path().join("test.txt").is_dir());
}

#[tokio::test]
async fn streams_plain_and_encrypted_files_without_requiring_a_full_buffer() {
    let source = b"binary\0\xff\x01\n".repeat(10_000);
    let mut plain = Cursor::new(Vec::new());
    assert_eq!(
        copy_stream(&mut plain, create_mock_stream(source.clone(), 127))
            .await
            .unwrap(),
        source.len() as u64
    );
    assert_eq!(plain.into_inner(), source);
    let crypto = Crypto::new();
    crypto.derive_dek("test".into(), "dGVzdC1zYWx0").unwrap();
    let (encrypted, nonce) = crypto
        .encrypt_streaming(create_mock_stream(source.clone(), 31))
        .unwrap();
    let decrypted = crypto.decrypt_streaming(encrypted, &nonce, 0).unwrap();
    let mut plain = Cursor::new(Vec::new());
    copy_stream(&mut plain, decrypted).await.unwrap();
    assert_eq!(plain.into_inner(), source);
}

#[tokio::test]
async fn reports_network_and_write_errors_instead_of_treating_partial_output_as_complete() {
    let stream: ByteStream = Box::pin(futures_util::stream::iter(vec![
        Ok(bytes::Bytes::from_static(b"partial")),
        Err(std::io::Error::other("network failed")),
    ]));
    let mut writer = Cursor::new(Vec::new());
    assert!(copy_stream(&mut writer, stream).await.is_err());
    struct BrokenWriter;
    impl Write for BrokenWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(
        copy_stream(&mut BrokenWriter, create_mock_stream(vec![1; 20], 3))
            .await
            .is_err()
    );
}

#[cfg(not(target_os = "android"))]
#[tokio::test]
async fn cancellation_removes_only_incomplete_files_and_keeps_completed_files() {
    let directory = tempfile::tempdir().unwrap();
    let (file, committed_path) = create_local_file(directory.path(), "done.txt").unwrap();
    let mut output = PendingExportFile {
        file: Some(file),
        destination: FilePath::Path(committed_path.clone()),
        committed: false,
    };
    output
        .file
        .as_mut()
        .unwrap()
        .write_all(b"complete")
        .unwrap();
    output.commit().unwrap();
    let (file, canceled_path) = create_local_file(directory.path(), "canceled.txt").unwrap();
    let path = canceled_path.clone();
    let (ready, started) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let mut output = PendingExportFile {
            file: Some(file),
            destination: FilePath::Path(path),
            committed: false,
        };
        output.file.as_mut().unwrap().write_all(b"partial").unwrap();
        let _ = ready.send(());
        std::future::pending::<()>().await;
        drop(output);
    });
    started.await.unwrap();
    task.abort();
    let _ = task.await;
    assert!(!canceled_path.exists());
    assert_eq!(std::fs::read(committed_path).unwrap(), b"complete");
}

#[test]
fn batch_progress_uses_bytes_handles_empty_files_and_reserves_final_completion() {
    assert_eq!(batch_percent(0, 900, 50, 1000, 0, 2), 45);
    assert_eq!(batch_percent(900, 100, 50, 1000, 1, 2), 95);
    assert_eq!(batch_percent(0, 0, 100, 0, 0, 2), 50);
    assert_eq!(batch_percent(0, 0, 100, 0, 1, 2), 99);
    assert_eq!(
        batch_percent(0, u64::MAX, 100, u128::from(u64::MAX), 0, 1),
        99
    );
}
