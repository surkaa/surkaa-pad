use super::*;

fn location(directory: bool) -> ExportLocation {
    ExportLocation {
        path: FilePath::Path(std::path::PathBuf::from("downloads").join("sample.txt")),
        directory,
    }
}

#[test]
fn handles_are_unique_and_unknown_handles_cannot_resolve_arbitrary_paths() {
    let registry = ExportLocations::default();
    let first = registry.register(location(false)).unwrap();
    let second = registry.register(location(true)).unwrap();
    assert_ne!(first, second);
    assert_eq!(hex::decode(&first).unwrap().len(), 16);
    assert!(!registry.get(&first).unwrap().directory);
    assert!(registry.get(&second).unwrap().directory);
    for unknown in [
        "",
        "downloads/sample.txt",
        "https://example.com",
        "content://other/document",
    ] {
        assert!(registry.get(unknown).is_err());
    }
}

#[test]
fn old_locations_are_bounded_and_expiration_does_not_change_new_locations() {
    let registry = ExportLocations::default();
    let first = registry.register(location(false)).unwrap();
    for _ in 0..MAX_EXPORT_LOCATIONS - 1 {
        registry.register(location(true)).unwrap();
    }
    assert!(registry.get(&first).is_ok());
    let last = registry.register(location(false)).unwrap();
    assert!(registry.get(&first).is_err());
    assert!(!registry.get(&last).unwrap().directory);
    assert_eq!(registry.0.lock().unwrap().len(), MAX_EXPORT_LOCATIONS);
}

#[test]
fn completion_event_contains_only_the_handle_not_the_saved_path() {
    let event = AttachmentProcessEvent::CompletedWithDownloadLocation("export-handle".into());
    assert_eq!(
        serde_json::to_value(event).unwrap(),
        serde_json::json!({
            "event": "completedWithDownloadLocation", "data": "export-handle"
        })
    );
}

#[cfg(not(target_os = "android"))]
#[test]
fn single_download_opens_parent_directory_while_batch_opens_selected_directory() {
    assert_eq!(
        local_directory(&location(false)).unwrap(),
        std::path::PathBuf::from("downloads")
    );
    assert_eq!(
        local_directory(&location(true)).unwrap(),
        std::path::PathBuf::from("downloads").join("sample.txt")
    );
    let remote = ExportLocation {
        path: FilePath::Url("https://example.com".parse().unwrap()),
        directory: true,
    };
    assert!(local_directory(&remote).is_err());
}
