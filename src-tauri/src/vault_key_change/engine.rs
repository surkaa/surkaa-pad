use super::inventory::MAX_GCM_BYTES;
use super::progress::{FileProgress, Notify};
use super::storage::{byte_stream, read_limited_stream, RekeyStorage};
use super::stream_digest::StreamDigest;
use super::types::{
    Checkpoint, Encoding, Journal, KeyChangeFileStep, KeyChangeProgress, VerifiedStage,
};
use super::{error, Result};
use crate::cryptos::Crypto;
use crate::object_locations::ObjectLocations;
use crate::stream::ByteStream;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use zeroize::Zeroizing;

pub(super) async fn digest(mut stream: ByteStream) -> Result<(u64, String)> {
    let mut sha = Sha256::new();
    let mut size = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(error)?;
        size = size
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| error("对象大小溢出"))?;
        sha.update(&chunk);
    }
    Ok((size, hex::encode(sha.finalize())))
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
) -> Result<ByteStream> {
    match encoding {
        Encoding::Ctr { nonce } => crypto.decrypt_streaming(stream, nonce, 0).map_err(error),
        _ => Ok(byte_stream(
            crypto
                .decrypt(&read_limited_stream(stream, key, MAX_GCM_BYTES).await?)
                .map_err(error)?,
        )),
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
    while checkpoint.next < checkpoint.items.len() {
        if paused() {
            return Err(error("已暂停改密，保留了检查点，可稍后继续"));
        }
        let index = checkpoint.next;
        let item = checkpoint.items[index].clone();
        let stage_key = ObjectLocations::vault_key_change_stage(&journal.transaction_id, index);
        let backup_key = ObjectLocations::vault_key_change_backup(&journal.transaction_id, index);
        let base = progress(checkpoint, "");
        let step = |phase: &str, action, size| {
            let mut base = base.clone();
            base.phase = phase.into();
            FileProgress::new(base, action, size, notify.clone())
        };
        if checkpoint.staged.is_none() {
            notify(progress(checkpoint, "encrypting"));
            if storage.head(&item.source.key).await?.as_ref() != Some(&item.source) {
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
            let source_digest = StreamDigest::default();
            let source = source_digest.wrap(writing.wrap(storage.read(&item.source.key).await?));
            let (stream, size, expected_plain_hash) = match &item.encoding {
                Encoding::Ctr { nonce } => {
                    let plain_digest = StreamDigest::default();
                    let plain =
                        plain_digest.wrap(old.decrypt_streaming(source, nonce, 0).map_err(error)?);
                    // CTR 加解密对称。新盐保证是全新的密钥，同一 nonce 在不同密钥下复用安全，
                    // 因而无需同时改写大附件的 IV 与 Manifest。不得用于相同密钥的原地重加密。
                    let encrypted = new.decrypt_streaming(plain, nonce, 0).map_err(error)?;
                    (encrypted, item.source.size, PlainHash::Stream(plain_digest))
                }
                _ => {
                    let mut plaintext = Zeroizing::new(
                        old.decrypt(
                            &read_limited_stream(source, &item.source.key, MAX_GCM_BYTES).await?,
                        )
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
                    let expected = hex::encode(Sha256::digest(&plaintext));
                    let encrypted = new.encrypt(&plaintext).map_err(error)?;
                    let size = encrypted.len() as u64;
                    (byte_stream(encrypted), size, PlainHash::Known(expected))
                }
            };
            // 以实际生成并上传的密文为基准，不能把读回的数据自己当作校验基准。
            let written_digest = StreamDigest::default();
            storage
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
            let checking = step("verifying", KeyChangeFileStep::CheckingStageContent, size);
            let readback_digest = StreamDigest::default();
            // 同一次读回中校验密文和解密内容；两种哈希均须与生成时的基准一致。
            let stream = readback_digest.wrap(checking.wrap(storage.read(&stage_key).await?));
            let (_, actual_plain_hash) =
                digest(plain_stream(stream, &stage_key, new, &item.encoding).await?).await?;
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
            });
            journal.update(new, checkpoint)?;
            storage.checkpoint(&journal.encode()?).await?;
        }

        let staged = checkpoint
            .staged
            .as_ref()
            .ok_or_else(|| error("缺少已验证的临时密文"))?;
        notify(progress(checkpoint, "committing"));
        let current = match storage.head(&item.source.key).await? {
            Some(entry) => {
                let checking = step("committing", KeyChangeFileStep::CheckingTarget, entry.size);
                let result = digest(checking.wrap(storage.read(&item.source.key).await?)).await?;
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
        if !already_committed {
            let checking = step("committing", KeyChangeFileStep::CheckingStage, staged.size);
            if digest(checking.wrap(storage.read(&stage_key).await?)).await?
                != (staged.size, staged.new_cipher_hash.clone())
            {
                return Err(error("临时密文已损坏，停止替换数据"));
            }
            checking.finish();
            // 原密文备份在替换前读回校验；验证目标与持久保存检查点后才删除。
            if storage.head(&backup_key).await?.is_none() {
                if current.is_none() {
                    return Err(error("原对象及旧密文备份均缺失，停止以免丢失数据"));
                }
                let copying = step("committing", KeyChangeFileStep::BackingUp, item.source.size);
                storage
                    .copy(&item.source.key, &backup_key, item.source.size, &copying)
                    .await?;
                copying.finish();
            }
            let checking = step(
                "committing",
                KeyChangeFileStep::CheckingBackup,
                item.source.size,
            );
            if digest(checking.wrap(storage.read(&backup_key).await?)).await?
                != (item.source.size, staged.old_cipher_hash.clone())
            {
                return Err(error("旧密文备份校验失败，未替换数据"));
            }
            checking.finish();
        }
        let etag = if already_committed
            && storage
                .head(&item.source.key)
                .await?
                .is_some_and(|e| !e.etag.is_empty())
        {
            storage
                .head(&item.source.key)
                .await?
                .ok_or_else(|| error("目标消失"))?
                .etag
        } else {
            let copying = step("committing", KeyChangeFileStep::WritingTarget, staged.size);
            let etag = storage
                .copy(&stage_key, &item.source.key, staged.size, &copying)
                .await?;
            copying.finish();
            etag
        };
        let checking = step(
            "committing",
            KeyChangeFileStep::CheckingCommitted,
            staged.size,
        );
        if digest(checking.wrap(storage.read(&item.source.key).await?)).await?
            != (staged.size, staged.new_cipher_hash.clone())
        {
            return Err(error("目标对象校验失败，保留临时密文供恢复"));
        }
        checking.finish();
        checkpoint.new_etags.insert(item.source.key, etag);
        checkpoint.next += 1;
        checkpoint.staged = None;
        journal.update(new, checkpoint)?;
        storage.checkpoint(&journal.encode()?).await?;
        storage.delete(&stage_key).await?;
        storage.delete(&backup_key).await?;
        notify(progress(checkpoint, "completedObject"));
    }
    Ok(())
}
