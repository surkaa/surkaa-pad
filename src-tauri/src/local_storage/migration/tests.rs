use super::plan::{
    build_plan, build_plan_for_resume, directory_size, resolve_request, validate_path_relationship,
    MigrationRequest,
};
use super::transfer::{copy_entries, verify_entries};
use super::*;
use crate::local_storage::required_space_with_margin;
use std::path::PathBuf;
use tokio::sync::mpsc;

fn manager(temp_dir: &tempfile::TempDir) -> LocalStorageManager {
    LocalStorageManager::new(
        crate::app_config::AppConfigStore::in_memory(crate::app_config::AppConfig::default()),
        temp_dir.path().join("local-data"),
    )
}

#[tokio::test]
async fn custom_plan_appends_los_and_counts_objects() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().join("source");
    let target_base = temp_dir.path().join("target");
    std::fs::create_dir_all(&target_base).unwrap();
    let source = LocalObjectStore::new(source_dir);
    source
        .save_bytes("nested/object-a", b"manifest")
        .await
        .unwrap();
    source
        .save_bytes("nested/object-b", b"attachment")
        .await
        .unwrap();

    let request = resolve_request(&manager(&temp_dir), Some(display_path(&target_base))).unwrap();
    let plan = build_plan(&source, request).await.unwrap().public();

    assert_eq!(PathBuf::from(plan.target_path), target_base.join("los"));
    assert_eq!(plan.total_files, 2);
    assert_eq!(plan.total_bytes, 18);
}

#[tokio::test]
async fn streaming_copy_preserves_nested_objects_and_etags() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source = LocalObjectStore::new(temp_dir.path().join("source"));
    let target = LocalObjectStore::new(temp_dir.path().join("target"));
    source
        .save_bytes("nested/object-a", b"manifest-data")
        .await
        .unwrap();
    source
        .save_bytes("nested/object-b", &vec![7; 1024 * 1024])
        .await
        .unwrap();
    let entries = source.get_all_entries().await.unwrap();
    let (sender, _receiver) = mpsc::unbounded_channel();
    let sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(sender);

    copy_entries(&source, &target, &entries, sender.clone())
        .await
        .unwrap();
    verify_entries(&source, &target, &entries, sender)
        .await
        .unwrap();

    for entry in entries {
        assert_eq!(
            source.get(&entry.key).await.unwrap(),
            target.get(&entry.key).await.unwrap()
        );
        assert_eq!(
            source.get_data(&entry.key).await.unwrap(),
            target.get_data(&entry.key).await.unwrap()
        );
    }
}

#[tokio::test]
async fn verification_rejects_same_size_corruption() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source = LocalObjectStore::new(temp_dir.path().join("source"));
    let target = LocalObjectStore::new(temp_dir.path().join("target"));
    source.save_bytes("nested/object", b"source").await.unwrap();
    let entries = source.get_all_entries().await.unwrap();
    target
        .save_stream_with_etag(
            "nested/object",
            &entries[0].etag,
            Box::pin(futures_util::stream::once(async {
                Ok(bytes::Bytes::from_static(b"target"))
            })),
        )
        .await
        .unwrap();
    let (sender, _receiver) = mpsc::unbounded_channel();
    let sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(sender);

    assert!(matches!(
        verify_entries(&source, &target, &entries, sender).await,
        Err(LocalStorageMigrationError::VerificationFailed { .. })
    ));
}

#[test]
fn rejects_relative_custom_path_and_overlapping_roots() {
    let temp_dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        resolve_request(&manager(&temp_dir), Some("relative".into())),
        Err(LocalStorageMigrationError::RelativePath)
    ));
    let source = temp_dir.path().join("source");
    assert!(matches!(
        validate_path_relationship(&source, &source.join("los")),
        Err(LocalStorageMigrationError::OverlappingPath)
    ));
}

#[test]
fn required_space_includes_safety_margin() {
    assert_eq!(required_space_with_margin(0), 0);
    assert_eq!(
        required_space_with_margin(100),
        100 + crate::local_storage::MINIMUM_FREE_SPACE_MARGIN
    );
    let large = 100 * 1024 * 1024 * 1024u64;
    assert_eq!(required_space_with_margin(large), large + large / 20);
}

#[test]
fn directory_size_counts_existing_staging_files() {
    let temp_dir = tempfile::tempdir().unwrap();
    let nested = temp_dir.path().join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(temp_dir.path().join("object.data"), vec![1; 512]).unwrap();
    std::fs::write(nested.join("object.md5"), vec![2; 32]).unwrap();

    assert_eq!(directory_size(temp_dir.path()).unwrap(), 544);
    assert_eq!(directory_size(&temp_dir.path().join("missing")).unwrap(), 0);
}

#[tokio::test]
async fn forced_copy_to_custom_location_switches_only_after_verification() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = manager(&temp_dir);
    let source_root = temp_dir.path().join("source");
    let target_base = temp_dir.path().join("target-base");
    std::fs::create_dir_all(&target_base).unwrap();
    let source = LocalObjectStore::new(source_root.clone());
    source
        .save_bytes("nested/object-a", b"manifest")
        .await
        .unwrap();
    source
        .save_bytes("nested/object-b", &vec![9; 2 * 1024 * 1024])
        .await
        .unwrap();
    let (sender, _receiver) = mpsc::unbounded_channel();
    let sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(sender);

    execute_migration(
        source,
        manager.clone(),
        sender,
        Some(display_path(&target_base)),
        false,
    )
    .await
    .unwrap();

    let target = LocalObjectStore::new(target_base.join("los"));
    assert!(!source_root.exists());
    assert_eq!(
        target.get_data("nested/object-a").await.unwrap(),
        b"manifest"
    );
    assert_eq!(
        target.get_size("nested/object-b").await.unwrap(),
        Some(2 * 1024 * 1024)
    );
    assert_eq!(
        manager.configured_location(),
        LocalStorageLocation::Custom {
            base_path: dunce::simplified(&target_base.canonicalize().unwrap()).to_path_buf(),
        }
    );
    assert!(manager.pending_migration().is_none());
}

#[tokio::test]
async fn pending_migration_with_completed_target_is_verified_and_resumed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = manager(&temp_dir);
    let source_root = temp_dir.path().join("source");
    let target_base = temp_dir.path().join("target-base");
    let target_root = target_base.join("los");
    std::fs::create_dir_all(&target_base).unwrap();
    let source = LocalObjectStore::new(source_root.clone());
    source
        .save_bytes("nested/object", b"resume-data")
        .await
        .unwrap();
    let target = LocalObjectStore::new(target_root.clone());
    let entries = source.get_all_entries().await.unwrap();
    let (copy_sender, _copy_receiver) = mpsc::unbounded_channel();
    let copy_sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(copy_sender);
    copy_entries(&source, &target, &entries, copy_sender)
        .await
        .unwrap();
    let location = LocalStorageLocation::Custom {
        base_path: target_base.canonicalize().unwrap(),
    };
    manager
        .config()
        .begin_local_storage_migration(PendingLocalStorageMigration::new(
            source_root.clone(),
            target_root,
            target_base.join("los.migrating"),
            location.clone(),
        ))
        .unwrap();
    let pending = manager.pending_migration().unwrap();
    let resumed_plan = build_plan_for_resume(
        &source,
        MigrationRequest {
            location: location.clone(),
            target_root: pending.target_root().to_path_buf(),
        },
        Some(&pending),
    )
    .await
    .unwrap();
    assert_eq!(resumed_plan.required_bytes, 0);
    let (sender, _receiver) = mpsc::unbounded_channel();
    let sender: Arc<dyn MessageSender<LocalStorageMigrationEvent>> = Arc::new(sender);

    execute_migration(source, manager.clone(), sender, None, false)
        .await
        .unwrap();

    assert!(!source_root.exists());
    assert_eq!(manager.configured_location(), location);
    assert!(manager.pending_migration().is_none());
}
