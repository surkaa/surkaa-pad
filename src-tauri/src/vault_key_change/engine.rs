use super::diagnostics::{self, Diagnostics};
use super::inventory::MAX_GCM_BYTES;
use super::progress::{FileProgress, Notify};
use super::storage::{byte_stream, read_limited_stream, RekeyStorage};
use super::stream_digest::StreamDigest;
use super::types::{
    Checkpoint, CloudStageEvidence, Encoding, Entry, Journal, KeyChangeFilePath, KeyChangeFileStep,
    KeyChangeProgress, VerifiedStage,
};
use super::{error, Result};
use crate::cryptos::Crypto;
use crate::object_locations::ObjectLocations;
use crate::stream::ByteStream;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use zeroize::Zeroizing;

#[cfg(test)]
pub(super) async fn digest(stream: ByteStream) -> Result<(u64, String)> {
    digest_observed(stream, None).await
}

async fn digest_observed(
    mut stream: ByteStream,
    diagnostics: Option<&Diagnostics>,
) -> Result<(u64, String)> {
    let mut sha = Sha256::new();
    let mut size = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(error)?;
        size = size
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| error("对象大小溢出"))?;
        diagnostics::hash(diagnostics, || sha.update(&chunk));
    }
    Ok((
        size,
        diagnostics::hash(diagnostics, || hex::encode(sha.finalize())),
    ))
}

fn ctr_stream(
    crypto: &Crypto,
    stream: ByteStream,
    nonce: &[u8],
    diagnostics: Option<&Diagnostics>,
) -> Result<ByteStream> {
    match diagnostics {
        Some(diagnostics) => {
            let diagnostics = diagnostics.clone();
            crypto
                .decrypt_streaming_with_timing(stream, nonce, 0, move |elapsed| {
                    diagnostics.crypto(elapsed)
                })
                .map_err(error)
        }
        None => crypto.decrypt_streaming(stream, nonce, 0).map_err(error),
    }
}

enum PlainHash {
    Stream(StreamDigest),
    Known(String),
}

impl PlainHash {
    fn result(self) -> Result<String> {
        match self {
            Self::Stream(digest) => Ok(digest.result()?.1),
            Self::Known(hash) => Ok(hash),
        }
    }
}

async fn plain_stream(
    stream: ByteStream,
    key: &str,
    crypto: &Crypto,
    encoding: &Encoding,
    diagnostics: Option<&Diagnostics>,
) -> Result<ByteStream> {
    match encoding {
        Encoding::Ctr { nonce } => ctr_stream(crypto, stream, nonce, diagnostics),
        _ => {
            let bytes = read_limited_stream(stream, key, MAX_GCM_BYTES).await?;
            Ok(byte_stream(
                diagnostics::crypto(diagnostics, || crypto.decrypt(&bytes)).map_err(error)?,
            ))
        }
    }
}

pub(super) fn progress(checkpoint: &Checkpoint, phase: &str) -> KeyChangeProgress {
    KeyChangeProgress {
        phase: phase.into(),
        completed_files: checkpoint.next as u32,
        total_files: checkpoint.items.len() as u32,
        completed_bytes: checkpoint.items[..checkpoint.next]
            .iter()
            .map(|i| i.source.size)
            .sum(),
        total_bytes: checkpoint.items.iter().map(|i| i.source.size).sum(),
        current_file: checkpoint
            .items
            .get(checkpoint.next)
            .map(|i| i.source.key.clone()),
        file_progress: None,
    }
}

/// 一次只处理一个对象，检查点持久后才清理它的临时密文。
/// 任何失败均保留恢复记录；调用方不得以取消任务为由删除记录或恢复普通访问。
pub(super) async fn run(
    storage: &dyn RekeyStorage,
    journal: &mut Journal,
    checkpoint: &mut Checkpoint,
    old: &Crypto,
    new: &Crypto,
    notify: impl Fn(KeyChangeProgress) + Send + Sync + 'static,
    paused: impl Fn() -> bool,
) -> Result<()> {
    let notify: Notify = Arc::new(notify);
    let diagnostics = storage.diagnostics();
    let mut migrated = journal.clone();
    let mut migrated_checkpoint = checkpoint.clone();
    if migrated.migrate(old, new, &mut migrated_checkpoint)? {
        storage.checkpoint(&migrated.encode()?).await?;
        *journal = migrated;
        *checkpoint = migrated_checkpoint;
    }
    while checkpoint.next < checkpoint.items.len() {
        if paused() {
            return Err(error("已暂停改密，保留了检查点，可稍后继续"));
        }
        let index = checkpoint.next;
        let item = checkpoint.items[index].clone();
        let kind = match item.encoding {
            Encoding::Ctr { .. } => "ctrAttachment",
            Encoding::Manifest => "diary",
            Encoding::Gcm => "gcmObject",
        };
        let mut file_timer = diagnostics
            .as_ref()
            .map(|d| d.file(index, kind, item.source.size));
        let mut phases = diagnostics.as_ref().map(|d| d.phases());
        let set_phase = |phases: &mut Option<diagnostics::PhaseTimer>, name| {
            if let Some(phases) = phases {
                phases.set(name);
            }
        };
        let stage_key = ObjectLocations::vault_key_change_stage(&journal.transaction_id, index);
        let backup_key = ObjectLocations::vault_key_change_backup(&journal.transaction_id, index);
        // 只有本次运行刚完成的校验才可走快速路径。即使检查点存有 ETag/CRC，
        // 恢复运行也必须重新检查正式对象、临时对象和备份，不信任旧进程的观察。
        let fresh_cloud = storage.supports_conditions() && checkpoint.staged.is_none();
        // CRC 是否存在要等备份复制完成后才知道；之前按可能需要读回备份估算。
        let work_path = std::sync::Mutex::new(if fresh_cloud {
            KeyChangeFilePath::CloudWithBackupRead
        } else {
            KeyChangeFilePath::Full
        });
        let base = progress(checkpoint, "");
        let step = |phase: &str, action, size| {
            let mut base = base.clone();
            base.phase = phase.into();
            FileProgress::new(
                base,
                action,
                size,
                notify.clone(),
                work_path.lock().expect("改密路径锁损坏").clone(),
            )
        };
        if checkpoint.staged.is_none() {
            set_phase(&mut phases, "sourceMetadata");
            notify(progress(checkpoint, "encrypting"));
            if !fresh_cloud && storage.head(&item.source.key).await?.as_ref() != Some(&item.source)
            {
                return Err(error(format!(
                    "源对象已变化: {}，不会覆盖",
                    item.source.key
                )));
            }
            let writing = step(
                "encrypting",
                KeyChangeFileStep::WritingStage,
                item.source.size,
            );
            set_phase(&mut phases, "writingStage");
            let source_digest = if fresh_cloud {
                StreamDigest::with_crc(diagnostics.as_ref())
            } else {
                StreamDigest::observed(diagnostics.as_ref())
            };
            let source = if fresh_cloud {
                storage
                    .read_if_match(&item.source.key, &item.source.etag)
                    .await?
            } else {
                storage.read(&item.source.key).await?
            };
            let source = source_digest.wrap(writing.wrap(source));
            let (stream, size, expected_plain_hash) = match &item.encoding {
                Encoding::Ctr { nonce } => {
                    let plain_digest = StreamDigest::observed(diagnostics.as_ref());
                    let plain =
                        plain_digest.wrap(ctr_stream(old, source, nonce, diagnostics.as_ref())?);
                    // CTR 加解密对称。新盐保证是全新的密钥，同一 nonce 在不同密钥下复用安全，
                    // 因而无需同时改写大附件的 IV 与 Manifest。不得用于相同密钥的原地重加密。
                    let encrypted = ctr_stream(new, plain, nonce, diagnostics.as_ref())?;
                    (encrypted, item.source.size, PlainHash::Stream(plain_digest))
                }
                _ => {
                    let bytes =
                        read_limited_stream(source, &item.source.key, MAX_GCM_BYTES).await?;
                    let mut plaintext = Zeroizing::new(
                        diagnostics::crypto(diagnostics.as_ref(), || old.decrypt(&bytes))
                            .map_err(error)?,
                    );
                    if matches!(item.encoding, Encoding::Manifest) {
                        let mut json: serde_json::Value =
                            serde_json::from_slice(&plaintext).map_err(error)?;
                        let diary_id = json["id"]
                            .as_str()
                            .ok_or_else(|| error("日记缺少 ID"))?
                            .to_owned();
                        let attachments = json["attachments"]
                            .as_array_mut()
                            .ok_or_else(|| error("日记缺少附件列表"))?;
                        for attachment in attachments {
                            let id = attachment["id"]
                                .as_str()
                                .ok_or_else(|| error("附件缺少 ID"))?;
                            if let Some(etag) = checkpoint
                                .new_etags
                                .get(&ObjectLocations::diary_attachment(&diary_id, id))
                            {
                                attachment["etag"] = etag.clone().into();
                            }
                        }
                        *plaintext = serde_json::to_vec(&json).map_err(error)?;
                    }
                    let expected = diagnostics::hash(diagnostics.as_ref(), || {
                        hex::encode(Sha256::digest(&plaintext))
                    });
                    let encrypted =
                        diagnostics::crypto(diagnostics.as_ref(), || new.encrypt(&plaintext))
                            .map_err(error)?;
                    let size = encrypted.len() as u64;
                    (byte_stream(encrypted), size, PlainHash::Known(expected))
                }
            };
            // 以实际生成并上传的密文为基准，不能把读回的数据自己当作校验基准。
            let written_digest = StreamDigest::observed(diagnostics.as_ref());
            let stage_etag = storage
                .write(&stage_key, size, written_digest.wrap(stream))
                .await?;
            let (old_size, old_cipher_hash) = source_digest.result()?;
            let (written_size, expected_cipher_hash) = written_digest.result()?;
            if old_size != item.source.size || written_size != size {
                return Err(error("改密数据流大小发生变化，未替换原对象"));
            }
            let expected_plain_hash = expected_plain_hash.result()?;
            writing.finish();
            notify(progress(checkpoint, "verifying"));
            set_phase(&mut phases, "checkingStageContent");
            let checking = step("verifying", KeyChangeFileStep::CheckingStageContent, size);
            let readback_digest = StreamDigest::observed(diagnostics.as_ref());
            // 同一次读回中校验密文和解密内容；两种哈希均须与生成时的基准一致。
            let stream = if fresh_cloud {
                storage.read_if_match(&stage_key, &stage_etag).await?
            } else {
                storage.read(&stage_key).await?
            };
            let stream = readback_digest.wrap(checking.wrap(stream));
            let (_, actual_plain_hash) = digest_observed(
                plain_stream(
                    stream,
                    &stage_key,
                    new,
                    &item.encoding,
                    diagnostics.as_ref(),
                )
                .await?,
                diagnostics.as_ref(),
            )
            .await?;
            let (actual_size, new_cipher_hash) = readback_digest.result()?;
            if actual_plain_hash != expected_plain_hash
                || actual_size != size
                || new_cipher_hash != expected_cipher_hash
            {
                return Err(error("新密文读回校验失败，未替换原对象"));
            }
            checking.finish();
            checkpoint.staged = Some(VerifiedStage {
                size,
                old_cipher_hash,
                new_cipher_hash,
                cloud: if fresh_cloud {
                    Some(CloudStageEvidence {
                        etag: stage_etag,
                        old_crc64: source_digest.crc64()?,
                    })
                } else {
                    None
                },
            });
            set_phase(&mut phases, "checkpoint");
            journal.update(new, checkpoint)?;
            storage.checkpoint(&journal.encode()?).await?;
        }

        let staged = checkpoint
            .staged
            .as_ref()
            .ok_or_else(|| error("缺少已验证的临时密文"))?;
        notify(progress(checkpoint, "committing"));
        let etag = if fresh_cloud {
            let evidence = staged
                .cloud
                .as_ref()
                .ok_or_else(|| error("缺少云端校验证据"))?;
            set_phase(&mut phases, "backingUp");
            let copying = step("committing", KeyChangeFileStep::BackingUp, item.source.size);
            let backup = storage
                .copy_if_match(&item.source, &backup_key, true)
                .await?;
            copying.finish();
            set_phase(&mut phases, "checkingBackup");
            match backup.crc64 {
                Some(crc) if crc == evidence.old_crc64 => {
                    *work_path.lock().expect("改密路径锁损坏") = KeyChangeFilePath::CloudFast;
                }
                Some(_) => return Err(error("旧密文备份 CRC 校验失败，未替换数据")),
                None => {
                    // CopyObject 不保证 CRC64 存在。缺失时读回完整备份比对本机源 SHA，
                    // 不能拿两个服务端元数据互相比对就宣称内容已经验证。
                    let checking = step(
                        "committing",
                        KeyChangeFileStep::CheckingBackup,
                        item.source.size,
                    );
                    if digest_observed(
                        checking.wrap(
                            storage
                                .read_if_match(&backup_key, &backup.entry.etag)
                                .await?,
                        ),
                        diagnostics.as_ref(),
                    )
                    .await?
                        != (item.source.size, staged.old_cipher_hash.clone())
                    {
                        return Err(error("旧密文备份校验失败，未替换数据"));
                    }
                    checking.finish();
                }
            }
            set_phase(&mut phases, "targetMetadata");
            if storage.head(&item.source.key).await?.as_ref() != Some(&item.source) {
                return Err(error("正式对象已变化，拒绝覆盖并保留恢复资料"));
            }
            // 该 HEAD 不是对目标的原子写锁；仍要求其他设备退出且本机业务冻结。
            set_phase(&mut phases, "writingTarget");
            let copying = step("committing", KeyChangeFileStep::WritingTarget, staged.size);
            let copied = storage
                .copy_if_match(
                    &Entry {
                        key: stage_key.clone(),
                        size: staged.size,
                        etag: evidence.etag.clone(),
                    },
                    &item.source.key,
                    false,
                )
                .await?;
            copying.finish();
            copied.entry.etag
        } else {
            set_phase(&mut phases, "checkingTarget");
            let current_entry = storage.head(&item.source.key).await?;
            let current = match &current_entry {
                Some(entry) => {
                    let checking =
                        step("committing", KeyChangeFileStep::CheckingTarget, entry.size);
                    let result = digest_observed(
                        checking.wrap(read_entry(storage, entry).await?),
                        diagnostics.as_ref(),
                    )
                    .await?;
                    if result != (staged.size, staged.new_cipher_hash.clone())
                        && result != (item.source.size, staged.old_cipher_hash.clone())
                    {
                        return Err(error(format!(
                            "目标对象变化，拒绝覆盖: {}",
                            item.source.key
                        )));
                    }
                    checking.finish();
                    Some(result)
                }
                None => None, // 本地替换数据/ETag 的中断窗口，由已验证的新密文修复。
            };
            let already_committed =
                current.as_ref() == Some(&(staged.size, staged.new_cipher_hash.clone()));
            if already_committed && current_entry.as_ref().is_some_and(|e| !e.etag.is_empty()) {
                *work_path.lock().expect("改密路径锁损坏") = KeyChangeFilePath::AlreadyCommitted;
            }
            let mut recovered_etag = None;
            if !already_committed {
                set_phase(&mut phases, "checkingStage");
                let stage_entry = if storage.supports_conditions() {
                    Some(
                        storage
                            .head(&stage_key)
                            .await?
                            .ok_or_else(|| error("临时密文不存在"))?,
                    )
                } else {
                    None
                };
                let checking = step("committing", KeyChangeFileStep::CheckingStage, staged.size);
                if digest_observed(
                    checking.wrap(match &stage_entry {
                        Some(entry) => read_entry(storage, entry).await?,
                        None => storage.read(&stage_key).await?,
                    }),
                    diagnostics.as_ref(),
                )
                .await?
                    != (staged.size, staged.new_cipher_hash.clone())
                {
                    return Err(error("临时密文已损坏，停止替换数据"));
                }
                checking.finish();
                set_phase(&mut phases, "backingUp");
                // 原密文备份在替换前读回校验；验证目标与持久保存检查点后才删除。
                let mut backup_entry = storage.head(&backup_key).await?;
                if backup_entry.is_none() {
                    if current.is_none() {
                        return Err(error("原对象及旧密文备份均缺失，停止以免丢失数据"));
                    }
                    let copying =
                        step("committing", KeyChangeFileStep::BackingUp, item.source.size);
                    if storage.supports_conditions() {
                        backup_entry = Some(
                            storage
                                .copy_if_match(
                                    current_entry
                                        .as_ref()
                                        .ok_or_else(|| error("原对象不存在"))?,
                                    &backup_key,
                                    true,
                                )
                                .await?
                                .entry,
                        );
                    } else {
                        storage
                            .copy(&item.source.key, &backup_key, item.source.size, &copying)
                            .await?;
                    }
                    copying.finish();
                }
                let checking = step(
                    "committing",
                    KeyChangeFileStep::CheckingBackup,
                    item.source.size,
                );
                set_phase(&mut phases, "checkingBackup");
                if digest_observed(
                    checking.wrap(match &backup_entry {
                        Some(entry) => read_entry(storage, entry).await?,
                        None => storage.read(&backup_key).await?,
                    }),
                    diagnostics.as_ref(),
                )
                .await?
                    != (item.source.size, staged.old_cipher_hash.clone())
                {
                    return Err(error("旧密文备份校验失败，未替换数据"));
                }
                checking.finish();
                // 恢复路径的临时密文已完整核对 SHA，再以同一源 ETag 条件提交。
                if let Some(stage_entry) = stage_entry {
                    if storage.head(&item.source.key).await? != current_entry {
                        return Err(error("恢复过程中正式对象发生变化，拒绝覆盖"));
                    }
                    set_phase(&mut phases, "writingTarget");
                    let copying = step("committing", KeyChangeFileStep::WritingTarget, staged.size);
                    let copied = storage
                        .copy_if_match(&stage_entry, &item.source.key, false)
                        .await?;
                    copying.finish();
                    recovered_etag = Some(copied.entry.etag);
                    // 统一交给下方最终读回/持久检查点，不提前记作已完成。
                }
            }
            set_phase(&mut phases, "writingTarget");
            if !already_committed && storage.supports_conditions() {
                recovered_etag.ok_or_else(|| error("缺少复制后的 ETag"))?
            } else if already_committed
                && current_entry.as_ref().is_some_and(|e| !e.etag.is_empty())
            {
                current_entry.as_ref().unwrap().etag.clone()
            } else {
                let copying = step("committing", KeyChangeFileStep::WritingTarget, staged.size);
                let etag = storage
                    .copy(&stage_key, &item.source.key, staged.size, &copying)
                    .await?;
                copying.finish();
                etag
            }
        };
        let checking = step(
            "committing",
            KeyChangeFileStep::CheckingCommitted,
            staged.size,
        );
        set_phase(&mut phases, "checkingCommitted");
        if digest_observed(
            checking.wrap(if storage.supports_conditions() {
                storage.read_if_match(&item.source.key, &etag).await?
            } else {
                storage.read(&item.source.key).await?
            }),
            diagnostics.as_ref(),
        )
        .await?
            != (staged.size, staged.new_cipher_hash.clone())
        {
            return Err(error("目标对象校验失败，保留临时密文供恢复"));
        }
        checking.finish();
        checkpoint.new_etags.insert(item.source.key, etag);
        checkpoint.next += 1;
        checkpoint.staged = None;
        if let Some(diagnostics) = &diagnostics {
            diagnostics.completed(checkpoint.next);
        }
        set_phase(&mut phases, "checkpoint");
        journal.update(new, checkpoint)?;
        storage.checkpoint(&journal.encode()?).await?;
        set_phase(&mut phases, "cleanup");
        storage.delete(&stage_key).await?;
        storage.delete(&backup_key).await?;
        notify(progress(checkpoint, "completedObject"));
        if let Some(phases) = &mut phases {
            phases.complete();
        }
        if let Some(timer) = &mut file_timer {
            timer.completed = true;
        }
    }
    Ok(())
}

async fn read_entry(storage: &dyn RekeyStorage, entry: &Entry) -> Result<ByteStream> {
    if storage.supports_conditions() {
        storage.read_if_match(&entry.key, &entry.etag).await
    } else {
        storage.read(&entry.key).await
    }
}
