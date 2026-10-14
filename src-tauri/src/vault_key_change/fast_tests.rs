use super::*;

#[tokio::test]
async fn cloud_engine_reports_actual_path_and_uses_full_check_after_lost_commit_response() {
    for omit_crc in [false, true] {
        let store = MemoryStore::default();
        store.0.lock().unwrap().read_chunk_size = Some(1024 * 1024);
        let (old, ob) = crypto("old");
        let (new, nb) = crypto("new");
        let expected = fixture(&store, &old, 10 * 1024 * 1024 + 17).await;
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (mut journal, mut cp) =
            Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
        {
            let mut data = store.0.lock().unwrap();
            data.cloud = true;
            data.omit_crc = omit_crc;
            data.journal = Some(journal.encode().unwrap());
        }
        let events = Arc::new(Mutex::new(vec![]));
        let saved = events.clone();
        let fault_store = store.clone();
        assert!(run(
            &store,
            &mut journal,
            &mut cp,
            &old,
            &new,
            move |p| {
                if p.file_progress.as_ref().is_some_and(|file| {
                    file.step == KeyChangeFileStep::WritingTarget && file.completed_points == 0
                }) {
                    let mut data = fault_store.0.lock().unwrap();
                    data.fail_at = Some(data.calls + 1);
                    data.fail_after = true;
                }
                saved.lock().unwrap().push(p);
            },
            || false
        )
        .await
        .is_err());
        let target_path = if omit_crc {
            KeyChangeFilePath::CloudWithBackupRead
        } else {
            KeyChangeFilePath::CloudFast
        };
        {
            let events = events.lock().unwrap();
            assert!(events.iter().any(|p| p
                .file_progress
                .as_ref()
                .is_some_and(|file| file.step == KeyChangeFileStep::WritingTarget
                    && file.path == target_path)));
            assert!(events
                .iter()
                .filter_map(|p| p.file_progress.as_ref())
                .filter(|file| file.step == KeyChangeFileStep::WritingStage
                    || file.step == KeyChangeFileStep::CheckingStageContent)
                .all(|file| file.path == KeyChangeFilePath::CloudWithBackupRead));
            assert_eq!(
                events.iter().any(|p| p
                    .file_progress
                    .as_ref()
                    .is_some_and(|file| file.step == KeyChangeFileStep::CheckingBackup)),
                omit_crc
            );
        }
        let encoded = {
            let mut data = store.0.lock().unwrap();
            data.fail_at = None;
            data.journal.clone().unwrap()
        };
        let mut journal = Journal::parse(&encoded).unwrap();
        let (old, new, mut cp) = journal.open("new".into()).unwrap();
        assert!(cp.staged.is_some());
        let resumed = Arc::new(Mutex::new(vec![]));
        let saved = resumed.clone();
        run(
            &store,
            &mut journal,
            &mut cp,
            &old,
            &new,
            move |p| saved.lock().unwrap().push(p),
            || false,
        )
        .await
        .unwrap();
        assert!(resumed.lock().unwrap().iter().any(|p| p
            .file_progress
            .as_ref()
            .is_some_and(|file| file.step == KeyChangeFileStep::CheckingTarget
                && file.path == KeyChangeFilePath::Full)));
        assert!(resumed.lock().unwrap().iter().any(|p| p
            .file_progress
            .as_ref()
            .is_some_and(|file| file.step == KeyChangeFileStep::CheckingCommitted
                && file.path == KeyChangeFilePath::AlreadyCommitted)));
        assert_data(&store, &new, &expected).await;
    }
}

#[tokio::test]
async fn cloud_fast_path_reads_only_source_stage_and_final_or_backup_fallback() {
    for omit_crc in [false, true] {
        let store = MemoryStore::default();
        let (old, ob) = crypto("old");
        let (new, nb) = crypto("new");
        let expected = fixture(&store, &old, 4097).await;
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (mut journal, mut cp) =
            Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
        {
            let mut data = store.0.lock().unwrap();
            data.cloud = true;
            data.omit_crc = omit_crc;
            data.read_counts.clear();
        }
        run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
            .await
            .unwrap();
        {
            let data = store.0.lock().unwrap();
            for (index, item) in cp.items.iter().enumerate() {
                assert_eq!(
                    data.read_counts[&item.source.key], 2,
                    "one old GET and one final GET"
                );
                let stage = ObjectLocations::vault_key_change_stage(&journal.transaction_id, index);
                let backup =
                    ObjectLocations::vault_key_change_backup(&journal.transaction_id, index);
                assert_eq!(data.read_counts[&stage], 1);
                assert_eq!(
                    data.read_counts.get(&backup).copied().unwrap_or_default(),
                    usize::from(omit_crc)
                );
                assert!(!data.objects.contains_key(&stage));
                assert!(!data.objects.contains_key(&backup));
            }
            assert_eq!(data.conditional_copies.len(), cp.items.len() * 2);
        }
        assert_data(&store, &new, &expected).await;
    }
}

#[tokio::test]
async fn cloud_every_io_boundary_and_lost_response_can_resume_from_durable_journal() {
    for omit_crc in [false, true] {
        let (old, ob) = crypto("old");
        let (new, nb) = crypto("new");
        let baseline = MemoryStore::default();
        let expected = fixture(&baseline, &old, 4097).await;
        let (_, items) = inventory(&baseline, &old).await.unwrap();
        let (journal, cp) =
            Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
        let objects = baseline.0.lock().unwrap().objects.clone();
        {
            let mut data = baseline.0.lock().unwrap();
            data.cloud = true;
            data.omit_crc = omit_crc;
            data.calls = 0;
        }
        run(
            &baseline,
            &mut journal.clone(),
            &mut cp.clone(),
            &old,
            &new,
            |_| {},
            || false,
        )
        .await
        .unwrap();
        let operations = baseline.0.lock().unwrap().calls;
        assert!(operations > 30);
        for fail_at in 1..=operations {
            for fail_after in [false, true] {
                let store = MemoryStore::default();
                {
                    let mut data = store.0.lock().unwrap();
                    data.cloud = true;
                    data.omit_crc = omit_crc;
                    data.objects = objects.clone();
                    data.journal = Some(journal.encode().unwrap());
                    data.fail_at = Some(fail_at);
                    data.fail_after = fail_after;
                }
                let _ = run(
                    &store,
                    &mut journal.clone(),
                    &mut cp.clone(),
                    &old,
                    &new,
                    |_| {},
                    || false,
                )
                .await;
                let saved = {
                    let mut data = store.0.lock().unwrap();
                    data.fail_at = None;
                    data.journal.clone().unwrap()
                };
                let mut recovered = Journal::parse(&saved).unwrap();
                let (old, new, mut cp) = recovered.open("new".into()).unwrap();
                run(
                    &store,
                    &mut recovered,
                    &mut cp,
                    &old,
                    &new,
                    |_| {},
                    || false,
                )
                .await
                .unwrap_or_else(|e| {
                    panic!("omit_crc={omit_crc} boundary={fail_at} after={fail_after}: {e}")
                });
                assert_eq!(cp.next, cp.items.len());
                assert_data(&store, &new, &expected).await;
            }
        }
    }
}

#[tokio::test]
async fn cloud_bad_backup_crc_stops_before_overwrite_and_resume_uses_full_hash() {
    let store = MemoryStore::default();
    let (old, ob) = crypto("old");
    let (new, nb) = crypto("new");
    let expected = fixture(&store, &old, 4097).await;
    let (_, items) = inventory(&store, &old).await.unwrap();
    let (mut journal, mut cp) =
        Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
    let source = cp.items[0].source.key.clone();
    let original = store.0.lock().unwrap().objects[&source].clone();
    {
        let mut data = store.0.lock().unwrap();
        data.cloud = true;
        data.bad_crc = true;
        data.read_counts.clear();
    }
    let error = run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("CRC"));
    let backup = ObjectLocations::vault_key_change_backup(&journal.transaction_id, 0);
    {
        let data = store.0.lock().unwrap();
        assert_eq!(data.objects[&source], original);
        assert_eq!(data.objects[&backup], original);
        assert_eq!(data.conditional_copies.len(), 1);
        assert_eq!(cp.next, 0);
    }
    let saved = store.0.lock().unwrap().journal.clone().unwrap();
    let mut journal = Journal::parse(&saved).unwrap();
    let (old, new, mut cp) = journal.open("new".into()).unwrap();
    // 恢复时即使 CRC 仍异常也完整读取备份 SHA；持久化的 cloud 字段不能绕过它。
    let notify_store = store.clone();
    run(
        &store,
        &mut journal,
        &mut cp,
        &old,
        &new,
        move |p| {
            if p.completed_files > 0 {
                notify_store.0.lock().unwrap().bad_crc = false;
            }
        },
        || false,
    )
    .await
    .unwrap();
    assert_eq!(store.0.lock().unwrap().read_counts[&backup], 1);
    assert_data(&store, &new, &expected).await;
}

#[tokio::test]
async fn cloud_detects_source_stage_and_final_changes_without_marking_complete() {
    for fault in [
        CopyFault::SourceChangesBeforeBackup,
        CopyFault::SourceChangesAfterBackup,
        CopyFault::StageChangesBeforeCommit,
        CopyFault::FinalCopyCorrupt,
    ] {
        let store = MemoryStore::default();
        let (old, ob) = crypto("old");
        let (new, nb) = crypto("new");
        fixture(&store, &old, 4097).await;
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (mut journal, mut cp) =
            Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
        let source = cp.items[0].source.key.clone();
        let original = store.0.lock().unwrap().objects[&source].clone();
        {
            let mut data = store.0.lock().unwrap();
            data.cloud = true;
            data.copy_fault = Some(fault);
        }
        assert!(
            run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
                .await
                .is_err()
        );
        assert_eq!(cp.next, 0);
        let stage = ObjectLocations::vault_key_change_stage(&journal.transaction_id, 0);
        let backup = ObjectLocations::vault_key_change_backup(&journal.transaction_id, 0);
        let saved = {
            let data = store.0.lock().unwrap();
            assert!(data.objects.contains_key(&stage));
            if !matches!(fault, CopyFault::SourceChangesBeforeBackup) {
                assert_eq!(data.objects[&backup], original);
            }
            if matches!(fault, CopyFault::StageChangesBeforeCommit) {
                assert_eq!(data.objects[&source], original);
            }
            data.journal.clone().unwrap()
        };
        let mut journal = Journal::parse(&saved).unwrap();
        let (old, new, mut cp) = journal.open("new".into()).unwrap();
        let before = store.0.lock().unwrap().objects.clone();
        assert!(
            run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
                .await
                .is_err()
        );
        assert_eq!(
            store.0.lock().unwrap().objects,
            before,
            "unknown or corrupt objects must not be overwritten on resume"
        );
    }
}

#[tokio::test]
async fn fresh_cloud_does_not_overwrite_existing_backup_and_resume_checks_its_content() {
    for (valid, ignore_forbid_overwrite) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let store = MemoryStore::default();
        let (old, ob) = crypto("old");
        let (new, nb) = crypto("new");
        let expected = fixture(&store, &old, 4097).await;
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (mut journal, mut cp) =
            Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
        let source = cp.items[0].source.key.clone();
        let backup = ObjectLocations::vault_key_change_backup(&journal.transaction_id, 0);
        let original = store.0.lock().unwrap().objects[&source].clone();
        let bytes = if valid {
            original.clone()
        } else {
            b"unrelated recovery data".to_vec()
        };
        {
            let mut data = store.0.lock().unwrap();
            data.cloud = true;
            data.ignore_forbid_overwrite = ignore_forbid_overwrite;
            data.objects.insert(backup.clone(), bytes.clone());
        }
        assert!(
            run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
                .await
                .unwrap_err()
                .to_string()
                .contains("备份已存在")
        );
        assert_eq!(store.0.lock().unwrap().objects[&backup], bytes);
        assert_eq!(store.0.lock().unwrap().objects[&source], original);
        let saved = store.0.lock().unwrap().journal.clone().unwrap();
        let mut journal = Journal::parse(&saved).unwrap();
        let (old, new, mut cp) = journal.open("new".into()).unwrap();
        let result = run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false).await;
        if valid {
            result.unwrap();
            assert_data(&store, &new, &expected).await;
        } else {
            assert!(result.is_err());
            assert_eq!(store.0.lock().unwrap().objects[&backup], bytes);
            assert_eq!(cp.next, 0);
        }
    }
}

fn legacy(journal: &Journal, cp: &Checkpoint, old: &Crypto, new: &Crypto) -> Journal {
    use sha2::{Digest, Sha256};
    let mut journal = journal.clone();
    journal.version = 1;
    let context = Sha256::digest(
        serde_json::to_vec(&(
            "surkaa-pad:key-change:v1",
            1_u32,
            &journal.transaction_id,
            journal.remote,
            &journal.old_bootstrap,
            &journal.new_bootstrap,
        ))
        .unwrap(),
    );
    journal.old_key_under_new = new.wrap_peer(old, &context).unwrap();
    let mut value = serde_json::to_value(cp).unwrap();
    value["header_hash"] = hex::encode(context).into();
    if let Some(staged) = value["staged"].as_object_mut() {
        staged.remove("cloud");
    }
    journal.encrypted_checkpoint = new.encrypt(&serde_json::to_vec(&value).unwrap()).unwrap();
    journal
}

#[tokio::test]
async fn cloud_stream_errors_at_source_stage_and_final_are_resumable_without_false_completion() {
    for object in ["source", "stage", "final"] {
        for fault in [ReadFault::Disconnected, ReadFault::Truncated] {
            let store = MemoryStore::default();
            let (old, ob) = crypto("old");
            let (new, nb) = crypto("new");
            let expected = fixture(&store, &old, 4097).await;
            let (_, items) = inventory(&store, &old).await.unwrap();
            let (mut journal, mut cp) =
                Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
            let source = cp.items[0].source.key.clone();
            let stage = ObjectLocations::vault_key_change_stage(&journal.transaction_id, 0);
            let original = store.0.lock().unwrap().objects[&source].clone();
            {
                let mut data = store.0.lock().unwrap();
                data.cloud = true;
                data.journal = Some(journal.encode().unwrap());
                if object != "final" {
                    data.read_fault = Some((
                        if object == "source" {
                            source.clone()
                        } else {
                            stage.clone()
                        },
                        fault,
                    ));
                }
            }
            let notified = store.clone();
            let fault_source = source.clone();
            assert!(run(
                &store,
                &mut journal,
                &mut cp,
                &old,
                &new,
                move |p| {
                    if object == "final" && p.file_progress.is_none() && p.phase == "committing" {
                        notified.0.lock().unwrap().read_fault = Some((fault_source.clone(), fault));
                    }
                },
                || false
            )
            .await
            .is_err());
            assert_eq!(cp.next, 0);
            if object != "final" {
                assert_eq!(store.0.lock().unwrap().objects[&source], original);
            }
            let saved = {
                let mut data = store.0.lock().unwrap();
                data.read_fault = None;
                data.journal.clone().unwrap()
            };
            let mut journal = Journal::parse(&saved).unwrap();
            let (old, new, mut cp) = journal.open("new".into()).unwrap();
            run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
                .await
                .unwrap();
            assert_data(&store, &new, &expected).await;
        }
    }
}

#[tokio::test]
async fn migration_checkpoint_failure_preserves_v1_until_the_v2_copy_is_durable() {
    for after in [false, true] {
        let store = MemoryStore::default();
        let (old, ob) = crypto("old");
        let (new, nb) = crypto("new");
        let expected = fixture(&store, &old, 4097).await;
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (journal, mut cp) =
            Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
        let mut journal = legacy(&journal, &cp, &old, &new);
        cp = journal.checkpoint(&new).unwrap();
        let objects = store.0.lock().unwrap().objects.clone();
        {
            let mut data = store.0.lock().unwrap();
            data.cloud = true;
            data.journal = Some(journal.encode().unwrap());
            data.calls = 0;
            data.fail_at = Some(1);
            data.fail_after = after;
        }
        assert!(
            run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
                .await
                .is_err()
        );
        assert_eq!(journal.version, 1);
        assert_eq!(store.0.lock().unwrap().objects, objects);
        let saved = {
            let mut data = store.0.lock().unwrap();
            data.fail_at = None;
            data.journal.clone().unwrap()
        };
        let mut journal = Journal::parse(&saved).unwrap();
        assert_eq!(journal.version, if after { 2 } else { 1 });
        let (old, new, mut cp) = journal.open("new".into()).unwrap();
        run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
            .await
            .unwrap();
        assert_eq!(journal.version, 2);
        assert_data(&store, &new, &expected).await;
    }
}

#[tokio::test]
async fn v1_journal_migrates_with_rewrapped_key_and_preserves_full_recovery() {
    let store = MemoryStore::default();
    let (old, ob) = crypto("old");
    let (new, nb) = crypto("new");
    let expected = fixture(&store, &old, 4097).await;
    let (_, items) = inventory(&store, &old).await.unwrap();
    let (mut journal, mut cp) =
        Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
    // 在临时密文及其检查点已提交后暂停，构造真实 V1 认证上下文和缺失新字段的检查点。
    let stopping = store.clone();
    assert!(run(
        &store,
        &mut journal,
        &mut cp,
        &old,
        &new,
        move |p| {
            if p.phase == "committing" && p.file_progress.is_none() {
                let mut data = stopping.0.lock().unwrap();
                data.fail_at = Some(data.calls + 1);
            }
        },
        || false
    )
    .await
    .is_err());
    assert!(cp.staged.is_some());
    let legacy = legacy(&journal, &cp, &old, &new);
    let mut parsed = Journal::parse(&legacy.encode().unwrap()).unwrap();
    let (old, new, mut opened) = parsed.open("new".into()).unwrap();
    assert!(opened.staged.as_ref().unwrap().cloud.is_none());
    let envelope = parsed.old_key_under_new.clone();
    assert!(parsed.migrate(&old, &new, &mut opened).unwrap());
    assert_eq!(
        parsed.version,
        super::super::journal::CURRENT_KEY_CHANGE_VERSION
    );
    assert_ne!(parsed.old_key_under_new, envelope);
    assert!(parsed.open("new".into()).is_ok());
    assert!(parsed.open("old".into()).is_err());
    assert!(!parsed.migrate(&old, &new, &mut opened).unwrap());
    // 只篡改版本号而未重新封装密钥必须无法通过认证。
    let mut invalid = legacy.clone();
    invalid.version = 2;
    assert!(invalid.open("new".into()).is_err());
    for version in [0, 3, u32::MAX] {
        invalid.version = version;
        assert!(Journal::parse(&invalid.encode().unwrap()).is_err());
    }
    let mut parsed = Journal::parse(&legacy.encode().unwrap()).unwrap();
    let (old, new, mut cp) = parsed.open("new".into()).unwrap();
    {
        let mut data = store.0.lock().unwrap();
        data.cloud = true;
        data.fail_at = None;
        data.read_counts.clear();
        data.journal = Some(legacy.encode().unwrap());
    }
    run(&store, &mut parsed, &mut cp, &old, &new, |_| {}, || false)
        .await
        .unwrap();
    assert_eq!(parsed.version, 2);
    let backup = ObjectLocations::vault_key_change_backup(&parsed.transaction_id, 0);
    assert_eq!(
        store.0.lock().unwrap().read_counts[&backup],
        1,
        "V1 resume must fully read the backup"
    );
    assert_data(&store, &new, &expected).await;
}
