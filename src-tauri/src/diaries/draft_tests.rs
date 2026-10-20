use super::*;

fn crypto() -> Crypto {
    let crypto = Crypto::new();
    crypto
        .derive_dek("draft-test".into(), "ZHJhZnQtdGVzdC1zYWx0MTIzNA")
        .unwrap();
    crypto
}
fn content(text: &str) -> DiaryContent {
    text.into()
}

#[tokio::test]
async fn encrypted_draft_survives_restart_and_stale_receipt_cannot_delete_newer_content() {
    let dir = tempfile::tempdir().unwrap();
    let crypto = crypto();
    let repo = DiaryDraftRepository::new(dir.path().into(), crypto.clone());
    assert!(repo.read("123").await.unwrap().is_none());
    let first = repo
        .save("123", content("private diary text"))
        .await
        .unwrap();
    let bytes = tokio::fs::read(dir.path().join("123.enc")).await.unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("private diary text"));
    let second = repo.save("123", content("new text")).await.unwrap();
    assert_ne!(first, second);
    assert!(!repo.clear_if_revision("123", &first).await.unwrap());
    let restarted = DiaryDraftRepository::new(dir.path().into(), crypto);
    assert_eq!(
        restarted.read("123").await.unwrap().unwrap().content,
        content("new text")
    );
    assert!(restarted.has_pending().await.unwrap());
    assert_eq!(restarted.list_ids().await.unwrap(), vec!["123"]);
    assert!(restarted.clear_if_revision("123", &second).await.unwrap());
    assert!(!restarted.has_pending().await.unwrap());
    assert!(restarted.list_ids().await.unwrap().is_empty());
}

#[tokio::test]
async fn corrupt_draft_is_preserved_and_interrupted_temp_does_not_replace_valid_draft() {
    let dir = tempfile::tempdir().unwrap();
    let repo = DiaryDraftRepository::new(dir.path().into(), crypto());
    repo.save("123", content("valid")).await.unwrap();
    tokio::fs::write(dir.path().join("123.enc.tmp"), b"interrupted")
        .await
        .unwrap();
    assert_eq!(
        repo.read("123").await.unwrap().unwrap().content,
        content("valid")
    );
    tokio::fs::write(dir.path().join("123.enc"), b"broken")
        .await
        .unwrap();
    assert!(repo.read("123").await.is_err());
    assert!(repo.clear_if_revision("123", "whatever").await.is_err());
    assert_eq!(
        tokio::fs::read(dir.path().join("123.enc")).await.unwrap(),
        b"broken"
    );
}

#[tokio::test]
async fn failed_write_does_not_clear_old_draft_and_ids_cannot_escape_directory() {
    let dir = tempfile::tempdir().unwrap();
    let repo = DiaryDraftRepository::new(dir.path().into(), crypto());
    repo.save("123", content("valid")).await.unwrap();
    tokio::fs::create_dir(dir.path().join("123.enc.tmp"))
        .await
        .unwrap();
    assert!(repo.save("123", content("lost")).await.is_err());
    assert_eq!(
        repo.read("123").await.unwrap().unwrap().content,
        content("valid")
    );
    for id in ["", "../123", "a", "1/2"] {
        assert!(repo.save(id, content("invalid")).await.is_err());
        assert!(repo.read(id).await.is_err());
    }
}
