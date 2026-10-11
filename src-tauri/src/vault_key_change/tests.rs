use super::engine::{digest, run};
use super::inventory::inventory;
use super::storage::{byte_stream, read_limited, RekeyStorage, Storage};
use super::types::*;
use super::{error, Result};
use crate::attachments::AttachmentMeta;
use crate::caches::LocalObjectStore;
use crate::cryptos::{crypto_types::EncryptionAlgorithm, Crypto};
use crate::diaries::{DiaryContent, DiaryManifest};
use crate::object::OssClient;
use crate::object_locations::ObjectLocations;
use crate::state::AppState;
use crate::stream::ByteStream;
use crate::vault_bootstrap::{KeyDerivationParameters, VaultBootstrap, VAULT_VERIFIER_TEXT};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct MemoryStore(Arc<Mutex<Memory>>);

#[derive(Clone, Copy)]
enum ReadFault {
    Truncated,
    Disconnected,
}

#[derive(Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    reported_sizes: BTreeMap<String, u64>,
    journal: Option<String>,
    calls: usize,
    fail_at: Option<usize>,
    fail_after: bool,
    read_chunk_size: Option<usize>,
    read_counts: BTreeMap<String, usize>,
    read_fault: Option<(String, ReadFault)>,
}

impl MemoryStore {
    fn tick(&self, after: bool) -> Result<()> {
        let mut data = self.0.lock().unwrap();
        if !after {
            data.calls += 1;
        }
        if data.fail_at == Some(data.calls) && data.fail_after == after {
            return Err(error("模拟网络断开 / 进程退出"));
        }
        Ok(())
    }

    fn entry(key: &str, bytes: &[u8]) -> Entry {
        Entry {
            key: key.into(),
            size: bytes.len() as u64,
            etag: format!("{:X}", md5::compute(bytes)),
        }
    }
}

#[async_trait]
impl RekeyStorage for MemoryStore {
    async fn list(&self) -> Result<Vec<Entry>> {
        self.tick(false)?;
        let data = self.0.lock().unwrap();
        Ok(data
            .objects
            .iter()
            .map(|(k, v)| {
                let mut entry = Self::entry(k, v);
                if let Some(size) = data.reported_sizes.get(k) {
                    entry.size = *size;
                }
                entry
            })
            .collect())
    }
    async fn head(&self, key: &str) -> Result<Option<Entry>> {
        self.tick(false)?;
        Ok(self
            .0
            .lock()
            .unwrap()
            .objects
            .get(key)
            .map(|v| Self::entry(key, v)))
    }
    async fn read(&self, key: &str) -> Result<ByteStream> {
        self.tick(false)?;
        let (mut bytes, chunk_size, fault) = {
            let mut data = self.0.lock().unwrap();
            *data.read_counts.entry(key.into()).or_default() += 1;
            (
                data.objects
                    .get(key)
                    .cloned()
                    .ok_or_else(|| error("对象不存在"))?,
                data.read_chunk_size.unwrap_or(73),
                data.read_fault
                    .as_ref()
                    .filter(|(k, _)| k == key)
                    .map(|(_, fault)| *fault),
            )
        };
        if matches!(fault, Some(ReadFault::Truncated)) {
            bytes.pop();
        }
        // 故意跨 AES 块边界分块，不能只测试整块输入。
        let mut chunks: Vec<_> = bytes
            .chunks(chunk_size)
            .map(|c| Ok(bytes::Bytes::copy_from_slice(c)))
            .collect();
        if matches!(fault, Some(ReadFault::Disconnected)) {
            chunks.truncate(1);
            chunks.push(Err(std::io::Error::other("模拟读取中途断流")));
        }
        Ok(Box::pin(futures_util::stream::iter(chunks)))
    }
    async fn write(&self, key: &str, size: u64, stream: ByteStream) -> Result<String> {
        self.tick(false)?;
        let bytes = crate::stream::collect_data(stream).await.map_err(error)?;
        if size != bytes.len() as u64 {
            return Err(error("写入流大小不匹配"));
        }
        let etag = Self::entry(key, &bytes).etag;
        self.0.lock().unwrap().objects.insert(key.into(), bytes);
        // 模拟服务端已完成 PUT，但客户端没有收到确认。
        self.tick(true)?;
        Ok(etag)
    }
    async fn delete(&self, key: &str) -> Result<()> {
        self.tick(false)?;
        self.0.lock().unwrap().objects.remove(key);
        self.tick(true)
    }
    async fn checkpoint(&self, journal: &str) -> Result<()> {
        self.tick(false)?;
        self.0.lock().unwrap().journal = Some(journal.into());
        self.tick(true)
    }
}

#[tokio::test]
async fn reencrypt_reads_each_source_only_once_before_readback() {
    let store = MemoryStore::default();
    let (old, ob) = crypto("old");
    let (new, nb) = crypto("new");
    let expected = fixture(&store, &old, 4097).await;
    let (_, items) = inventory(&store, &old).await.unwrap();
    let (mut journal, mut cp) =
        Journal::create(&old, &new, ob, nb, false, items, Default::default()).unwrap();
    store.0.lock().unwrap().read_counts.clear();
    let notify_store = store.clone();
    run(
        &store,
        &mut journal,
        &mut cp,
        &old,
        &new,
        move |p| {
            if p.phase == "verifying" && p.file_progress.is_none() {
                let key = p.current_file.unwrap();
                assert_eq!(notify_store.0.lock().unwrap().read_counts[&key], 1);
            }
        },
        || false,
    )
    .await
    .unwrap();
    for (index, item) in cp.items.iter().enumerate() {
        let data = store.0.lock().unwrap();
        // 生成临时密文、替换前检查、复制旧备份、最终目标校验各读取一次。
        assert_eq!(data.read_counts[&item.source.key], 4);
        let stage = ObjectLocations::vault_key_change_stage(&journal.transaction_id, index);
        // 一次同时校验明文/密文、替换前检查、复制到目标各读取一次。
        assert_eq!(data.read_counts[&stage], 3);
    }
    assert_data(&store, &new, &expected).await;
}

#[tokio::test]
async fn interrupted_or_truncated_source_stream_keeps_original_and_resumes() {
    for fault in [ReadFault::Disconnected, ReadFault::Truncated] {
        let store = MemoryStore::default();
        let (old, ob) = crypto("old");
        let (new, nb) = crypto("new");
        let expected = fixture(&store, &old, 4097).await;
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (mut journal, mut cp) =
            Journal::create(&old, &new, ob, nb, false, items, Default::default()).unwrap();
        let index = cp
            .items
            .iter()
            .position(|i| matches!(i.encoding, Encoding::Ctr { .. }))
            .unwrap();
        let target = cp.items[index].source.key.clone();
        let original = store.0.lock().unwrap().objects[&target].clone();
        store.checkpoint(&journal.encode().unwrap()).await.unwrap();
        store.0.lock().unwrap().read_fault = Some((target.clone(), fault));
        assert!(
            run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
                .await
                .is_err()
        );
        assert_eq!(cp.next, index);
        assert!(cp.staged.is_none());
        assert_eq!(store.0.lock().unwrap().objects[&target], original);
        store.0.lock().unwrap().read_fault = None;
        let saved = store.0.lock().unwrap().journal.clone().unwrap();
        let mut journal = Journal::parse(&saved).unwrap();
        let (old, new, mut cp) = journal.open("new".into()).unwrap();
        run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
            .await
            .unwrap();
        assert_data(&store, &new, &expected).await;
    }
}

#[tokio::test]
async fn readback_rejects_different_gcm_ciphertext_even_with_identical_plaintext() {
    let store = MemoryStore::default();
    let (old, ob) = crypto("old");
    let (new, nb) = crypto("new");
    fixture(&store, &old, 129).await;
    let (_, items) = inventory(&store, &old).await.unwrap();
    let (mut journal, mut cp) =
        Journal::create(&old, &new, ob, nb, false, items, Default::default()).unwrap();
    let target = cp.items[0].source.key.clone();
    assert!(matches!(cp.items[0].encoding, Encoding::Gcm));
    let original = store.0.lock().unwrap().objects[&target].clone();
    let stage = ObjectLocations::vault_key_change_stage(&journal.transaction_id, 0);
    let notify_store = store.clone();
    let notify_new = new.clone();
    let result = run(
        &store,
        &mut journal,
        &mut cp,
        &old,
        &new,
        move |p| {
            if p.phase == "verifying" && p.file_progress.is_none() {
                // 使用新随机 nonce 重加密相同内容，模拟“明文正确但并非刚上传的密文”。
                let mut data = notify_store.0.lock().unwrap();
                let plaintext = notify_new.decrypt(&data.objects[&stage]).unwrap();
                let different = notify_new.encrypt(&plaintext).unwrap();
                assert_ne!(different, data.objects[&stage]);
                data.objects.insert(stage.clone(), different);
            }
        },
        || false,
    )
    .await;
    assert!(result.unwrap_err().message.contains("新密文读回校验失败"));
    assert_eq!(cp.next, 0);
    assert!(cp.staged.is_none());
    assert_eq!(store.0.lock().unwrap().objects[&target], original);
}

fn crypto(password: &str) -> (Crypto, VaultBootstrap) {
    let crypto = Crypto::new();
    let params = KeyDerivationParameters::new_random(1024).unwrap();
    crypto
        .derive_dek_with_parameters(password.into(), params.clone())
        .unwrap();
    let bootstrap = VaultBootstrap {
        schema_version: 1,
        vault_id: "0123456789abcdef0123456789abcdef".into(),
        kdf: params,
        encrypted_verifier: STANDARD
            .encode(crypto.encrypt(VAULT_VERIFIER_TEXT.as_bytes()).unwrap()),
    };
    (crypto, bootstrap)
}

async fn fixture(store: &dyn RekeyStorage, old: &Crypto, bytes: usize) -> Vec<u8> {
    let plain: Vec<u8> = (0..bytes).map(|i| (i % 251) as u8).collect();
    let (stream, nonce) = old.encrypt_streaming(byte_stream(plain.clone())).unwrap();
    let key = ObjectLocations::diary_attachment("123", "audio");
    let etag = store.write(&key, bytes as u64, stream).await.unwrap();
    let manifest = DiaryManifest {
        id: "123".into(),
        algorithm: EncryptionAlgorithm::Gcm,
        content: DiaryContent::from("测试日记 📖"),
        created: 12,
        updated: 34,
        version: crate::diaries::CURRENT_VERSION,
        attachments: vec![
            AttachmentMeta {
                id: "audio".into(),
                filename: "声音.mp3".into(),
                mimetype: "audio/mpeg".into(),
                size: bytes as u64,
                encrypted: true,
                algorithm: EncryptionAlgorithm::Ctr,
                nonce,
                etag: Some(etag),
                content_info: None,
            },
            AttachmentMeta {
                id: "plain".into(),
                filename: "公开.txt".into(),
                mimetype: "text/plain".into(),
                size: 5,
                encrypted: false,
                algorithm: EncryptionAlgorithm::Gcm,
                nonce: vec![],
                etag: None,
                content_info: None,
            },
        ],
    };
    for (key, plain) in [
        (
            ObjectLocations::diary_manifest("123"),
            serde_json::to_vec(&manifest).unwrap(),
        ),
        (
            ObjectLocations::ai_session_meta("456"),
            b"{\"title\":\"test\"}".to_vec(),
        ),
        (
            ObjectLocations::ai_session_message_block("456", 1, 0),
            b"[\"a\",\"b\"]".to_vec(),
        ),
        (
            ObjectLocations::synced_settings().into(),
            b"{\"version\":1}".to_vec(),
        ),
    ] {
        let cipher = old.encrypt(&plain).unwrap();
        store
            .write(&key, cipher.len() as u64, byte_stream(cipher))
            .await
            .unwrap();
    }
    store
        .write(
            &ObjectLocations::diary_attachment("123", "plain"),
            5,
            byte_stream(b"hello".to_vec()),
        )
        .await
        .unwrap();
    plain
}

async fn assert_data(store: &dyn RekeyStorage, new: &Crypto, expected: &[u8]) {
    let cipher = read_limited(store, &ObjectLocations::diary_manifest("123"), 1024 * 1024)
        .await
        .unwrap();
    let manifest: DiaryManifest = serde_json::from_slice(&new.decrypt(&cipher).unwrap()).unwrap();
    assert_eq!(manifest.updated, 34, "改密不得改写用户编辑时间");
    let key = ObjectLocations::diary_attachment("123", "audio");
    let plain = crate::stream::collect_data(
        new.decrypt_streaming(
            store.read(&key).await.unwrap(),
            &manifest.attachments[0].nonce,
            0,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(plain, expected);
    assert_eq!(
        manifest.attachments[0].etag,
        Some(store.head(&key).await.unwrap().unwrap().etag)
    );
    assert_eq!(
        read_limited(
            store,
            &ObjectLocations::diary_attachment("123", "plain"),
            10
        )
        .await
        .unwrap(),
        b"hello"
    );
    for key in [
        ObjectLocations::ai_session_meta("456"),
        ObjectLocations::ai_session_message_block("456", 1, 0),
        ObjectLocations::synced_settings().into(),
    ] {
        assert!(new
            .decrypt(&read_limited(store, &key, 1024).await.unwrap())
            .is_ok());
    }
}

#[tokio::test]
async fn every_io_boundary_can_resume_with_new_password() {
    let (old, old_bootstrap) = crypto("old");
    let (new, new_bootstrap) = crypto("new");
    let baseline = MemoryStore::default();
    let expected = fixture(&baseline, &old, 4097).await;
    let (_, items) = inventory(&baseline, &old).await.unwrap();
    let local = LocalEncryptedConfig {
        vault_verifier: Some(new.encrypt(VAULT_VERIFIER_TEXT.as_bytes()).unwrap()),
        ..Default::default()
    };
    let (journal, checkpoint) = Journal::create(
        &old,
        &new,
        old_bootstrap,
        new_bootstrap,
        false,
        items,
        local,
    )
    .unwrap();
    let original_objects = baseline.0.lock().unwrap().objects.clone();
    baseline.0.lock().unwrap().calls = 0;
    run(
        &baseline,
        &mut journal.clone(),
        &mut checkpoint.clone(),
        &old,
        &new,
        |_| {},
        || false,
    )
    .await
    .unwrap();
    let count = baseline.0.lock().unwrap().calls;
    assert!(count > 60);
    for fail_at in 1..=count {
        for fail_after in [false, true] {
            let store = MemoryStore::default();
            {
                let mut m = store.0.lock().unwrap();
                m.objects = original_objects.clone();
                m.journal = Some(journal.encode().unwrap());
                m.fail_at = Some(fail_at);
                m.fail_after = fail_after;
            }
            let _ = run(
                &store,
                &mut journal.clone(),
                &mut checkpoint.clone(),
                &old,
                &new,
                |_| {},
                || false,
            )
            .await;
            // 丢弃全部内存检查点，严格从最后持久化的记录重建。
            let saved = {
                let mut m = store.0.lock().unwrap();
                m.fail_at = None;
                m.journal.clone().unwrap()
            };
            let mut recovered = Journal::parse(&saved).unwrap();
            let (recovered_old, recovered_new, mut recovered_checkpoint) =
                recovered.open("new".into()).unwrap();
            run(
                &store,
                &mut recovered,
                &mut recovered_checkpoint,
                &recovered_old,
                &recovered_new,
                |_| {},
                || false,
            )
            .await
            .unwrap_or_else(|e| panic!("boundary {fail_at}, after={fail_after}: {e}"));
            assert_eq!(recovered_checkpoint.next, recovered_checkpoint.items.len());
            assert_data(&store, &new, &expected).await;
            let new_manifest = read_limited(&store, &ObjectLocations::diary_manifest("123"), 4096)
                .await
                .unwrap();
            assert!(old.decrypt(&new_manifest).is_err());
        }
    }
}

#[tokio::test]
async fn journal_rejects_wrong_password_modified_header_and_corrupt_ciphertext() {
    let (old, old_bootstrap) = crypto("old");
    let (new, new_bootstrap) = crypto("new");
    let (journal, _) = Journal::create(
        &old,
        &new,
        old_bootstrap,
        new_bootstrap,
        true,
        vec![],
        Default::default(),
    )
    .unwrap();
    assert!(journal.open("wrong".into()).is_err());
    assert!(
        journal.open("old".into()).is_err(),
        "旧密码不得通过云端日志获得新密钥"
    );
    assert!(journal.open("new".into()).is_ok());
    let mut bad = journal.clone();
    bad.remote = false;
    assert!(bad.open("new".into()).is_err());
    let mut bad = journal.clone();
    let replacement = if bad.transaction_id.starts_with('f') {
        "0"
    } else {
        "f"
    };
    bad.transaction_id.replace_range(0..1, replacement);
    assert!(bad.open("new".into()).is_err());
    let mut bad = journal.clone();
    bad.encrypted_checkpoint[15] ^= 1;
    assert!(bad.open("new".into()).is_err());
    let mut bad = journal;
    bad.old_key_under_new[16] ^= 1;
    assert!(bad.open("new".into()).is_err());
}

#[tokio::test]
async fn oversized_authenticated_objects_are_rejected_before_download() {
    let (old, _) = crypto("old");
    for key in [
        ObjectLocations::diary_manifest("123"),
        ObjectLocations::ai_session_meta("456"),
        ObjectLocations::ai_session_message_block("456", 0, 0),
        ObjectLocations::synced_settings().into(),
    ] {
        let store = MemoryStore::default();
        {
            let mut data = store.0.lock().unwrap();
            data.objects.insert(key.clone(), vec![0]);
            data.reported_sizes
                .insert(key, super::inventory::MAX_GCM_BYTES as u64 + 1);
        }
        assert!(inventory(&store, &old).await.is_err());
        let data = store.0.lock().unwrap();
        assert_eq!(data.calls, 1, "大对象应在 LIST 后拒绝，不能尝试下载");
        assert!(data.journal.is_none());
    }
}

#[tokio::test]
async fn bounded_reads_check_actual_stream_length_not_only_metadata() {
    let store = MemoryStore::default();
    store
        .0
        .lock()
        .unwrap()
        .objects
        .insert("sample".into(), vec![7; 74]);
    assert_eq!(read_limited(&store, "sample", 74).await.unwrap().len(), 74);
    assert!(read_limited(&store, "sample", 73).await.is_err());
}

#[tokio::test]
async fn preflight_rejects_orphan_missing_corrupt_unknown_and_pending_attachment_transaction() {
    let (old, _) = crypto("old");
    for case in 0..5 {
        let store = MemoryStore::default();
        fixture(&store, &old, 127).await;
        {
            let mut m = store.0.lock().unwrap();
            match case {
                0 => {
                    m.objects.insert(
                        ObjectLocations::diary_attachment("123", "orphan"),
                        vec![0; 2],
                    );
                }
                1 => {
                    m.objects
                        .remove(&ObjectLocations::diary_attachment("123", "audio"));
                }
                2 => {
                    m.objects
                        .get_mut(&ObjectLocations::diary_manifest("123"))
                        .unwrap()[13] ^= 1;
                }
                3 => {
                    m.objects.insert("unknown.enc".into(), vec![1]);
                }
                _ => {
                    m.objects.insert(
                        ObjectLocations::diary_attachment_backup("123", "audio"),
                        vec![1],
                    );
                }
            }
        }
        let before = store.0.lock().unwrap().objects.clone();
        assert!(inventory(&store, &old).await.is_err());
        assert_eq!(store.0.lock().unwrap().objects, before);
        assert!(store.0.lock().unwrap().journal.is_none());
    }
}

#[tokio::test]
async fn changed_source_and_corrupted_staging_are_never_overwritten() {
    let (old, ob) = crypto("old");
    let (new, nb) = crypto("new");
    let store = MemoryStore::default();
    fixture(&store, &old, 127).await;
    let (_, items) = inventory(&store, &old).await.unwrap();
    let (mut journal, mut checkpoint) =
        Journal::create(&old, &new, ob, nb, false, items, Default::default()).unwrap();
    let first = checkpoint.items[0].source.key.clone();
    store
        .0
        .lock()
        .unwrap()
        .objects
        .insert(first.clone(), b"changed".to_vec());
    assert!(run(
        &store,
        &mut journal,
        &mut checkpoint,
        &old,
        &new,
        |_| {},
        || false
    )
    .await
    .is_err());
    assert_eq!(store.0.lock().unwrap().objects[&first], b"changed");
    assert_eq!(checkpoint.next, 0);
}

#[tokio::test]
async fn local_streaming_rotation_and_restart_lock() {
    let temp = tempfile::tempdir().unwrap();
    let (old, ob) = crypto("same-password");
    let (new, nb) = crypto("same-password");
    let state = AppState::from_parts(
        old.clone(),
        OssClient::new(),
        LocalObjectStore::new(temp.path().into()),
    );
    state.persist_vault_bootstrap(ob.clone()).unwrap();
    let store = Storage {
        state: state.clone(),
        remote: false,
    };
    let expected = fixture(&store, &old, 30 * 1024 * 1024 + 17).await;
    let (_, items) = inventory(&store, &old).await.unwrap();
    let (mut journal, mut checkpoint) =
        Journal::create(&old, &new, ob, nb, false, items, Default::default()).unwrap();
    store.checkpoint(&journal.encode().unwrap()).await.unwrap();
    assert!(state.lock_storage_operation().await.is_err());
    assert!(state.try_lock_storage_mode_change().is_none());
    assert!(state.try_lock_key_change().is_some());
    assert!(run(
        &store,
        &mut journal,
        &mut checkpoint,
        &old,
        &new,
        |_| {},
        || true
    )
    .await
    .is_err());
    assert_eq!(checkpoint.next, 0);
    let events = Arc::new(Mutex::new(vec![]));
    let notify_events = events.clone();
    run(
        &store,
        &mut journal,
        &mut checkpoint,
        &old,
        &new,
        move |p| {
            notify_events.lock().unwrap().push(p);
        },
        || false,
    )
    .await
    .unwrap();
    for step in [
        KeyChangeFileStep::WritingStage,
        KeyChangeFileStep::CheckingStageContent,
        KeyChangeFileStep::CheckingTarget,
        KeyChangeFileStep::CheckingStage,
        KeyChangeFileStep::BackingUp,
        KeyChangeFileStep::CheckingBackup,
        KeyChangeFileStep::WritingTarget,
        KeyChangeFileStep::CheckingCommitted,
    ] {
        let captured = events.lock().unwrap();
        let points: Vec<_> = captured
            .iter()
            .filter_map(|p| p.file_progress.as_ref())
            .filter(|p| p.step == step)
            .map(|p| p.completed_points)
            .collect();
        assert_eq!(points, (0..=10).collect::<Vec<_>>(), "{step:?}");
    }
    assert_data(&store, &new, &expected).await;
    // 改密尚未最后确认前，活跃密钥和 bootstrap 不得提前切换。
    assert!(state
        .crypto()
        .decrypt(
            &STANDARD
                .decode(state.vault_bootstrap().unwrap().encrypted_verifier)
                .unwrap()
        )
        .is_ok());
    assert!(state.pending_key_change().is_some());
}

#[tokio::test]
async fn cloud_streaming_rotation_cleans_multipart_and_resumes() {
    let client = OssClient::from_env();
    let (client, guard) = crate::test_utils::TestOssGuard::new(client).await;
    client.require_unversioned_bucket().await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (old, ob) = crypto("old");
    let (new, nb) = crypto("new");
    let state = AppState::from_parts(
        old.clone(),
        client.clone(),
        LocalObjectStore::new(temp.path().into()),
    );
    let store = Storage {
        state,
        remote: true,
    };
    let expected = fixture(&store, &old, 12 * 1024 * 1024 + 17).await;
    let (_, items) = inventory(&store, &old).await.unwrap();
    let (mut journal, mut checkpoint) =
        Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
    store.checkpoint(&journal.encode().unwrap()).await.unwrap();
    let paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let notify_paused = paused.clone();
    let pause_after = checkpoint
        .items
        .iter()
        .position(|item| matches!(item.encoding, Encoding::Ctr { .. }))
        .unwrap() as u32
        + 1;
    assert!(pause_after < checkpoint.items.len() as u32);
    let events = Arc::new(Mutex::new(vec![]));
    let notify_events = events.clone();
    assert!(run(
        &store,
        &mut journal,
        &mut checkpoint,
        &old,
        &new,
        move |p| {
            if p.completed_files == pause_after {
                notify_paused.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            notify_events.lock().unwrap().push(p);
        },
        || paused.load(std::sync::atomic::Ordering::SeqCst)
    )
    .await
    .is_err());
    let steps = [
        KeyChangeFileStep::WritingStage,
        KeyChangeFileStep::CheckingStageContent,
        KeyChangeFileStep::CheckingBackup,
        KeyChangeFileStep::CheckingCommitted,
    ];
    for step in steps {
        let captured = events.lock().unwrap();
        let file_events: Vec<_> = captured
            .iter()
            .filter_map(|p| p.file_progress.as_ref())
            .filter(|p| p.step == step)
            .collect();
        assert!(file_events.len() >= 3, "{step:?} 应有真实流式进度");
        assert_eq!(file_events.first().unwrap().completed_points, 0);
        assert_eq!(file_events.last().unwrap().completed_points, 10);
        assert!(file_events
            .windows(2)
            .all(|pair| pair[0].completed_points < pair[1].completed_points));
    }
    let saved = read_limited(&store, ObjectLocations::vault_key_change(), 1024 * 1024)
        .await
        .unwrap();
    let mut journal = Journal::parse(std::str::from_utf8(&saved).unwrap()).unwrap();
    let (old, new, mut checkpoint) = journal.open("new".into()).unwrap();
    run(
        &store,
        &mut journal,
        &mut checkpoint,
        &old,
        &new,
        |_| {},
        || false,
    )
    .await
    .unwrap();
    assert_data(&store, &new, &expected).await;
    assert!(client.list_multipart_uploads("").await.unwrap().is_empty());
    for entry in store.list().await.unwrap() {
        assert!(!entry
            .key
            .starts_with(&ObjectLocations::vault_key_change_staging_prefix(
                &journal.transaction_id
            )));
    }
    guard.cleanup().await;
}

#[tokio::test]
async fn stream_digest_rejects_io_failure() {
    let stream: ByteStream = Box::pin(futures_util::stream::iter([
        Ok(bytes::Bytes::from_static(b"abc")),
        Err(std::io::Error::other("断流")),
    ]));
    assert!(digest(stream).await.is_err());
}

#[tokio::test]
async fn large_file_readback_corruption_and_lost_write_ack_never_report_success_and_resume() {
    for corrupt_readback in [true, false] {
        let store = MemoryStore::default();
        store.0.lock().unwrap().read_chunk_size = Some(256 * 1024);
        let (old, ob) = crypto("old");
        let (new, nb) = crypto("new");
        let expected = fixture(&store, &old, 10 * 1024 * 1024 + 17).await;
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (mut journal, mut cp) =
            Journal::create(&old, &new, ob, nb, false, items, Default::default()).unwrap();
        store.checkpoint(&journal.encode().unwrap()).await.unwrap();
        let index = cp
            .items
            .iter()
            .position(|i| matches!(i.encoding, Encoding::Ctr { .. }))
            .unwrap();
        let target = cp.items[index].source.key.clone();
        let original = store.0.lock().unwrap().objects[&target].clone();
        let stage = ObjectLocations::vault_key_change_stage(&journal.transaction_id, index);
        let events = Arc::new(Mutex::new(vec![]));
        let notify_events = events.clone();
        let notify_store = store.clone();
        let once = std::sync::atomic::AtomicBool::new(false);
        let result = run(
            &store,
            &mut journal,
            &mut cp,
            &old,
            &new,
            move |p| {
                if let Some(file) = &p.file_progress {
                    let inject = if corrupt_readback {
                        file.step == KeyChangeFileStep::CheckingStageContent
                            && file.completed_points == 0
                    } else {
                        file.step == KeyChangeFileStep::WritingStage && file.completed_points == 9
                    };
                    if inject && !once.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let mut data = notify_store.0.lock().unwrap();
                        if corrupt_readback {
                            data.objects.get_mut(&stage).unwrap()[20] ^= 1;
                        } else {
                            // 完整 PUT 已写入，但响应丢失：不能提前显示写入 100%。
                            data.fail_at = Some(data.calls);
                            data.fail_after = true;
                        }
                    }
                }
                notify_events.lock().unwrap().push(p);
            },
            || false,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(cp.next, index);
        assert_eq!(store.0.lock().unwrap().objects[&target], original);
        let failed_step = if corrupt_readback {
            KeyChangeFileStep::CheckingStageContent
        } else {
            KeyChangeFileStep::WritingStage
        };
        {
            let captured = events.lock().unwrap();
            let file_events: Vec<_> = captured
                .iter()
                .filter_map(|p| p.file_progress.as_ref())
                .filter(|p| p.step == failed_step)
                .collect();
            assert!(file_events.len() > 1);
            assert!(file_events
                .iter()
                .all(|p| p.completed_points < p.total_points));
        }
        store.0.lock().unwrap().fail_at = None;
        let encoded = store.0.lock().unwrap().journal.clone().unwrap();
        let mut journal = Journal::parse(&encoded).unwrap();
        let (old, new, mut cp) = journal.open("new".into()).unwrap();
        run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false)
            .await
            .unwrap();
        assert_data(&store, &new, &expected).await;
    }
}

fn persistent_state(temp: &std::path::Path) -> AppState {
    let config = crate::app_config::AppConfigStore::load(temp.join("app-state.json")).unwrap();
    let manager = crate::local_storage::LocalStorageManager::new(config.clone(), temp.into());
    AppState::new(
        temp.join("los"),
        crate::attachments::AttachmentServerHandle::for_test(),
        config,
        manager,
    )
}

fn channel() -> tauri::ipc::Channel<KeyChangeProgress> {
    tauri::ipc::Channel::new(|_| Ok(()))
}

fn local_config(old: &Crypto) -> LocalEncryptedConfig {
    LocalEncryptedConfig {
        vault_verifier: Some(old.encrypt(VAULT_VERIFIER_TEXT.as_bytes()).unwrap()),
        oss: Some(old.encrypt(b"{\"example\":\"local-only\"}").unwrap()),
        ai: Some(
            old.encrypt(b"{\"apiKey\":\"do-not-log-this-key\"}")
                .unwrap(),
        ),
    }
}

#[tokio::test]
async fn empty_vault_still_rotates_credentials_and_requires_final_ack() {
    let temp = tempfile::tempdir().unwrap();
    let state = persistent_state(temp.path());
    let (old, bootstrap) = crypto("old");
    state.crypto().adopt(&old).unwrap();
    state.persist_vault_bootstrap(bootstrap).unwrap();
    let plan = super::command::plan_change(
        &state,
        "old".into(),
        "new".into(),
        "new".into(),
        Some(2048),
        local_config(&old),
    )
    .await
    .unwrap();
    assert_eq!(
        (plan.total_files, plan.encrypted_files, plan.total_bytes),
        (0, 0, 0)
    );
    assert_eq!(plan.new_bootstrap.kdf.memory_cost_kib, 2048);
    let config = super::command::start_change(&state, plan.plan_id, channel())
        .await
        .unwrap();
    assert!(state.pending_key_change().is_some());
    assert!(state.lock_storage_operation().await.is_err());
    assert!(old
        .decrypt(config.vault_verifier.as_ref().unwrap())
        .is_err());
    assert_eq!(
        state
            .crypto()
            .decrypt(config.vault_verifier.as_ref().unwrap())
            .unwrap(),
        VAULT_VERIFIER_TEXT.as_bytes()
    );
    super::command::ack_change(&state, config).await.unwrap();
    assert!(state.pending_key_change().is_none());
    assert!(state.lock_storage_operation().await.is_ok());
}

#[tokio::test]
async fn complete_local_workflow_survives_restart_before_frontend_ack() {
    let temp = tempfile::tempdir().unwrap();
    let state = persistent_state(temp.path());
    let (old, ob) = crypto("old");
    state.crypto().adopt(&old).unwrap();
    state.persist_vault_bootstrap(ob.clone()).unwrap();
    let store = Storage {
        state: state.clone(),
        remote: false,
    };
    let expected = fixture(&store, &old, 2049).await;
    let original = local_config(&old);
    let plan = super::command::plan_change(
        &state,
        "old".into(),
        "new".into(),
        "new".into(),
        None,
        original.clone(),
    )
    .await
    .unwrap();
    assert_eq!(plan.total_files, 6);
    assert_eq!(plan.encrypted_files, 5);
    assert!(state.pending_key_change().is_none());
    assert_eq!(state.vault_bootstrap().unwrap(), ob, "统计不能切换密钥");
    let result = super::command::start_change(&state, plan.plan_id, channel())
        .await
        .unwrap();
    assert!(state.pending_key_change().is_some());
    assert_eq!(state.vault_bootstrap().unwrap(), plan.new_bootstrap);
    for data in [&result.vault_verifier, &result.oss, &result.ai] {
        assert!(old.decrypt(data.as_ref().unwrap()).is_err());
    }

    // 真正重新读取配置文件，丢弃活跃密钥、计划和检查点。
    let restarted = persistent_state(temp.path());
    assert!(!restarted.crypto().is_initialized().unwrap());
    assert!(restarted.lock_storage_operation().await.is_err());
    let resumed = super::command::resume_change(&restarted, "new".into(), channel())
        .await
        .unwrap();
    assert_eq!(resumed, result);
    let new = restarted.crypto();
    assert_eq!(
        new.decrypt(resumed.ai.as_ref().unwrap()).unwrap(),
        old.decrypt(original.ai.as_ref().unwrap()).unwrap()
    );
    assert_data(
        &Storage {
            state: restarted.clone(),
            remote: false,
        },
        &new,
        &expected,
    )
    .await;
    let mut bad = resumed.clone();
    bad.ai = None;
    assert!(super::command::ack_change(&restarted, bad).await.is_err());
    assert!(restarted.pending_key_change().is_some());
    super::command::ack_change(&restarted, resumed.clone())
        .await
        .unwrap();
    super::command::ack_change(&restarted, resumed)
        .await
        .unwrap();
    assert!(restarted.lock_storage_operation().await.is_ok());
    assert!(!temp.path().join("app-state.json.bak").exists());
    let final_state = persistent_state(temp.path());
    assert!(final_state.pending_key_change().is_none());
    let bootstrap = final_state.vault_bootstrap().unwrap();
    assert!(final_state
        .crypto()
        .derive_and_verify_bootstrap("old".into(), &bootstrap)
        .is_err());
    final_state
        .crypto()
        .derive_and_verify_bootstrap("new".into(), &bootstrap)
        .unwrap();
}

#[tokio::test]
async fn preflight_rejects_wrong_password_and_stale_plan_without_changing_data() {
    let temp = tempfile::tempdir().unwrap();
    let state = persistent_state(temp.path());
    let (old, ob) = crypto("old");
    state.crypto().adopt(&old).unwrap();
    state.persist_vault_bootstrap(ob).unwrap();
    let store = Storage {
        state: state.clone(),
        remote: false,
    };
    fixture(&store, &old, 123).await;
    for (password, confirmation) in [("wrong", "new"), ("old", "not-new")] {
        assert!(super::command::plan_change(
            &state,
            password.into(),
            "new".into(),
            confirmation.into(),
            None,
            local_config(&old)
        )
        .await
        .is_err());
        assert!(state.pending_key_change().is_none());
    }
    let plan = super::command::plan_change(
        &state,
        "old".into(),
        "new".into(),
        "new".into(),
        None,
        local_config(&old),
    )
    .await
    .unwrap();
    let changed = old.encrypt(b"changed-settings").unwrap();
    store
        .write(
            ObjectLocations::synced_settings(),
            changed.len() as u64,
            byte_stream(changed),
        )
        .await
        .unwrap();
    assert!(
        super::command::start_change(&state, plan.plan_id, channel())
            .await
            .is_err()
    );
    assert!(state.pending_key_change().is_none());
}

#[tokio::test]
async fn corrupt_stage_stops_before_replacement_and_missing_target_recovers_from_backup() {
    let (old, ob) = crypto("old");
    let (new, nb) = crypto("new");
    for corrupt in [true, false] {
        let store = MemoryStore::default();
        let expected = fixture(&store, &old, 257).await;
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (mut journal, mut cp) = Journal::create(
            &old,
            &new,
            ob.clone(),
            nb.clone(),
            false,
            items,
            Default::default(),
        )
        .unwrap();
        let stage = ObjectLocations::vault_key_change_stage(&journal.transaction_id, 0);
        let backup = ObjectLocations::vault_key_change_backup(&journal.transaction_id, 0);
        let target = cp.items[0].source.key.clone();
        let original = store.0.lock().unwrap().objects[&target].clone();
        let once = std::sync::atomic::AtomicBool::new(false);
        let notify_store = store.clone();
        let notify_stage = stage.clone();
        let notify_backup = backup.clone();
        let notify_target = target.clone();
        let notify_original = original.clone();
        let result = run(
            &store,
            &mut journal,
            &mut cp,
            &old,
            &new,
            move |p| {
                if p.phase == "committing" && !once.swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    let mut m = notify_store.0.lock().unwrap();
                    if corrupt {
                        m.objects.get_mut(&notify_stage).unwrap()[20] ^= 1;
                    } else {
                        m.objects
                            .insert(notify_backup.clone(), notify_original.clone());
                        m.objects.remove(&notify_target);
                    }
                }
            },
            || false,
        )
        .await;
        if corrupt {
            assert!(result.is_err());
            assert_eq!(store.0.lock().unwrap().objects[&target], original);
        } else {
            result.unwrap();
            assert_data(&store, &new, &expected).await;
        }
    }
}

#[tokio::test]
async fn queued_operation_is_rejected_after_key_change_record_is_saved() {
    let temp = tempfile::tempdir().unwrap();
    let state = persistent_state(temp.path());
    let exclusive = state.try_lock_key_change().unwrap();
    let operation = state.lock_storage_operation();
    tokio::pin!(operation);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), &mut operation)
            .await
            .is_err()
    );
    state
        .persist_key_change(Some("pending-record".into()))
        .unwrap();
    drop(exclusive);
    assert!(
        operation.await.is_err(),
        "排队中的普通操作也不能在改密暂停后执行"
    );
}

#[tokio::test]
async fn gcm_attachment_rotates_and_corrupt_backup_refuses_replacement() {
    let (old, ob) = crypto("old");
    let (new, nb) = crypto("new");
    for corrupt_backup in [false, true] {
        let store = MemoryStore::default();
        fixture(&store, &old, 129).await;
        let key = ObjectLocations::diary_attachment("123", "gcm");
        let cipher = old.encrypt(b"authenticated attachment").unwrap();
        let etag = store
            .write(&key, cipher.len() as u64, byte_stream(cipher))
            .await
            .unwrap();
        let manifest_key = ObjectLocations::diary_manifest("123");
        let mut manifest: DiaryManifest = serde_json::from_slice(
            &old.decrypt(&read_limited(&store, &manifest_key, 4096).await.unwrap())
                .unwrap(),
        )
        .unwrap();
        manifest.attachments.push(AttachmentMeta {
            id: "gcm".into(),
            filename: "gcm.bin".into(),
            mimetype: "application/octet-stream".into(),
            size: 24,
            encrypted: true,
            algorithm: EncryptionAlgorithm::Gcm,
            nonce: vec![],
            etag: Some(etag),
            content_info: None,
        });
        let cipher = old
            .encrypt(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        store
            .write(&manifest_key, cipher.len() as u64, byte_stream(cipher))
            .await
            .unwrap();
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (mut journal, mut cp) = Journal::create(
            &old,
            &new,
            ob.clone(),
            nb.clone(),
            false,
            items,
            Default::default(),
        )
        .unwrap();
        let target = cp.items[0].source.key.clone();
        let original = store.0.lock().unwrap().objects[&target].clone();
        if corrupt_backup {
            store.0.lock().unwrap().objects.insert(
                ObjectLocations::vault_key_change_backup(&journal.transaction_id, 0),
                b"broken".to_vec(),
            );
        }
        let result = run(&store, &mut journal, &mut cp, &old, &new, |_| {}, || false).await;
        if corrupt_backup {
            assert!(result.is_err());
            assert_eq!(store.0.lock().unwrap().objects[&target], original);
        } else {
            result.unwrap();
            assert_eq!(
                new.decrypt(&read_limited(&store, &key, 1024).await.unwrap())
                    .unwrap(),
                b"authenticated attachment"
            );
            let manifest: DiaryManifest = serde_json::from_slice(
                &new.decrypt(&read_limited(&store, &manifest_key, 4096).await.unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                manifest.attachments[2].etag,
                Some(store.head(&key).await.unwrap().unwrap().etag)
            );
        }
    }
}

fn cloud_config(old: &Crypto) -> LocalEncryptedConfig {
    let mut config = local_config(old);
    let json = serde_json::json!({
        "akid": std::env::var("ALIYUN_KEY").unwrap(), "aks": std::env::var("ALIYUN_SECRET").unwrap(),
        "bucket": std::env::var("ALIYUN_BUCKET_NAME").unwrap(), "endpoint": std::env::var("ALIYUN_ENDPOINT").unwrap(),
    });
    config.oss = Some(old.encrypt(&serde_json::to_vec(&json).unwrap()).unwrap());
    config
}

#[tokio::test]
async fn cloud_workflow_preserves_bootstrap_until_done_and_refreshes_other_device_locally() {
    let (client, guard) = crate::test_utils::TestOssGuard::new(OssClient::from_env()).await;
    let temp = tempfile::tempdir().unwrap();
    let other_temp = tempfile::tempdir().unwrap();
    let (old, ob) = crypto("old");
    let state = AppState::from_parts(
        old.snapshot().unwrap(),
        client.clone(),
        LocalObjectStore::new(temp.path().into()),
    );
    state.persist_vault_bootstrap(ob.clone()).unwrap();
    state.persist_remote_enabled(true).unwrap();
    state.set_remote_enabled(true);
    let cloud = Storage {
        state: state.clone(),
        remote: true,
    };
    let expected = fixture(&cloud, &old, 2051).await;
    client
        .upload_bytes(
            ObjectLocations::vault_bootstrap(),
            &serde_json::to_vec(&ob).unwrap(),
        )
        .await
        .unwrap();
    let cache = Storage {
        state: state.clone(),
        remote: false,
    };
    fixture(&cache, &old, 77).await;
    let other = AppState::from_parts(
        old.snapshot().unwrap(),
        client.clone(),
        LocalObjectStore::new(other_temp.path().into()),
    );
    other.persist_vault_bootstrap(ob.clone()).unwrap();
    other.persist_remote_enabled(true).unwrap();
    fixture(
        &Storage {
            state: other.clone(),
            remote: false,
        },
        &old,
        31,
    )
    .await;
    let mut other_config = cloud_config(&old);
    other_config.ai = Some(old.encrypt(b"other-device-api-key").unwrap());
    let plan = super::command::plan_change(
        &state,
        "old".into(),
        "new".into(),
        "new".into(),
        None,
        cloud_config(&old),
    )
    .await
    .unwrap();
    assert_eq!(plan.transfer_bytes, plan.encrypted_bytes * 14);
    let manager = state.key_change_manager();
    let pause_channel = tauri::ipc::Channel::new(move |body| {
        if let tauri::ipc::InvokeResponseBody::Json(json) = body {
            let p: serde_json::Value = serde_json::from_str(&json).unwrap();
            if p["phase"] == "completedObject" {
                manager.request_pause();
            }
        }
        Ok(())
    });
    assert!(
        super::command::start_change(&state, plan.plan_id, pause_channel)
            .await
            .is_err()
    );
    let raw = read_limited(&cloud, ObjectLocations::vault_bootstrap(), 16384)
        .await
        .unwrap();
    assert_eq!(
        VaultBootstrap::from_json(std::str::from_utf8(&raw).unwrap()).unwrap(),
        ob
    );
    assert!(super::command::sync_changed_key(
        &other,
        "new".into(),
        other_config.clone(),
        channel()
    )
    .await
    .is_err());
    let encoded = state.pending_key_change().unwrap();
    let journal = Journal::parse(&encoded).unwrap();
    assert!(journal.open("old".into()).is_err());

    // 恢复只清理本事务/当前替换目标的分片，不能误删其他对象的未完成上传。
    let (_, _, mut checkpoint) = journal.open("new".into()).unwrap();
    checkpoint.staged = Some(VerifiedStage {
        size: 0,
        old_cipher_hash: String::new(),
        new_cipher_hash: String::new(),
    });
    let target = checkpoint.items[checkpoint.next].source.key.clone();
    let stage = ObjectLocations::vault_key_change_stage(&journal.transaction_id, checkpoint.next);
    let unrelated = "unrelated-upload.bin";
    let stage_upload = client
        .initiate_multipart_upload(&stage, "application/octet-stream")
        .await
        .unwrap();
    let target_upload = client
        .initiate_multipart_upload(&target, "application/octet-stream")
        .await
        .unwrap();
    let unrelated_upload = client
        .initiate_multipart_upload(unrelated, "application/octet-stream")
        .await
        .unwrap();
    super::command::cleanup_interrupted_uploads(&cloud, &journal, &checkpoint)
        .await
        .unwrap();
    let remaining = client.list_multipart_uploads("").await.unwrap();
    assert!(!remaining
        .iter()
        .any(|(_, id)| id == &stage_upload || id == &target_upload));
    assert!(remaining.iter().any(|(_, id)| id == &unrelated_upload));
    // 云锁初次创建或最后清理的响应丢失，也应能凭本机记录安全重新取得云锁。
    client
        .delete(ObjectLocations::vault_key_change())
        .await
        .unwrap();
    let restarted = AppState::from_parts(
        Crypto::new(),
        client.clone(),
        LocalObjectStore::new(temp.path().into()),
    );
    restarted.persist_vault_bootstrap(ob.clone()).unwrap();
    restarted.persist_remote_enabled(true).unwrap();
    restarted.persist_key_change(Some(encoded)).unwrap();
    let config = super::command::resume_change(&restarted, "new".into(), channel())
        .await
        .unwrap();
    assert_data(
        &Storage {
            state: restarted.clone(),
            remote: true,
        },
        &restarted.crypto(),
        &expected,
    )
    .await;
    assert!(
        cache.list().await.unwrap().is_empty(),
        "旧云缓存不能混入新密钥数据"
    );
    let saved = restarted.pending_key_change().unwrap();
    let mut foreign = Journal::parse(&saved).unwrap();
    foreign.transaction_id = "ffffffffffffffffffffffffffffffff".into();
    client
        .upload_bytes(
            ObjectLocations::vault_key_change(),
            foreign.encode().unwrap().as_bytes(),
        )
        .await
        .unwrap();
    assert!(
        super::command::ack_change(&restarted, config.clone())
            .await
            .is_err(),
        "不能删除另一事务的云锁"
    );
    client
        .upload_bytes(ObjectLocations::vault_key_change(), saved.as_bytes())
        .await
        .unwrap();
    super::command::ack_change(&restarted, config)
        .await
        .unwrap();
    assert!(!client
        .object_exists(ObjectLocations::vault_key_change())
        .await
        .unwrap());
    let before = cloud.list().await.unwrap();

    assert!(super::command::sync_changed_key(
        &other,
        "wrong".into(),
        other_config.clone(),
        channel()
    )
    .await
    .is_err());
    assert!(other.pending_key_change().is_none());
    let rotated = super::command::sync_changed_key(&other, "new".into(), other_config, channel())
        .await
        .unwrap();
    assert!(
        other
            .crypto()
            .decrypt(rotated.ai.as_ref().unwrap())
            .unwrap()
            == b"other-device-api-key",
        "保留本机自己的配置，不复制发起设备的 API Key"
    );
    let other_record = other.pending_key_change().unwrap();
    let other_restart = AppState::from_parts(
        Crypto::new(),
        client.clone(),
        LocalObjectStore::new(other_temp.path().into()),
    );
    other_restart
        .persist_vault_bootstrap(other.vault_bootstrap().unwrap())
        .unwrap();
    other_restart
        .persist_key_change(Some(other_record))
        .unwrap();
    let resumed = super::command::resume_change(&other_restart, "new".into(), channel())
        .await
        .unwrap();
    assert_eq!(resumed, rotated);
    super::command::ack_change(&other_restart, resumed)
        .await
        .unwrap();
    assert!(Storage {
        state: other_restart,
        remote: false
    }
    .list()
    .await
    .unwrap()
    .is_empty());
    assert!(
        before == cloud.list().await.unwrap(),
        "其他设备只更新本机，不能重写已改密的云数据"
    );
    client
        .abort_multipart_upload(unrelated, &unrelated_upload)
        .await
        .unwrap();
    assert!(client.list_multipart_uploads("").await.unwrap().is_empty());
    guard.cleanup().await;
}
