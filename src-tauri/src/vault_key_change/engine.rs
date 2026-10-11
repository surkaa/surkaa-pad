use super::inventory::MAX_GCM_BYTES;
use super::storage::{byte_stream, read_limited, RekeyStorage};
use super::types::{Checkpoint, Encoding, Journal, KeyChangeProgress, VerifiedStage};
use super::{error, Result};
use crate::cryptos::Crypto;
use crate::object_locations::ObjectLocations;
use crate::stream::ByteStream;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
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

async fn plain_stream(
    storage: &dyn RekeyStorage,
    key: &str,
    crypto: &Crypto,
    encoding: &Encoding,
) -> Result<ByteStream> {
    match encoding {
        Encoding::Ctr { nonce } => crypto
            .decrypt_streaming(storage.read(key).await?, nonce, 0)
            .map_err(error),
        _ => Ok(byte_stream(
            crypto
                .decrypt(&read_limited(storage, key, MAX_GCM_BYTES).await?)
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
    notify: impl Fn(KeyChangeProgress),
    paused: impl Fn() -> bool,
) -> Result<()> {
    while checkpoint.next < checkpoint.items.len() {
        if paused() {
            return Err(error("已暂停改密，保留了检查点，可稍后继续"));
        }
        let index = checkpoint.next;
        let item = checkpoint.items[index].clone();
        let stage_key = ObjectLocations::vault_key_change_stage(&journal.transaction_id, index);
        let backup_key = ObjectLocations::vault_key_change_backup(&journal.transaction_id, index);
        if checkpoint.staged.is_none() {
            notify(progress(checkpoint, "encrypting"));
            if storage.head(&item.source.key).await?.as_ref() != Some(&item.source) {
                return Err(error(format!(
                    "源对象已变化: {}，不会覆盖",
                    item.source.key
                )));
            }
            let (old_size, old_cipher_hash) = digest(storage.read(&item.source.key).await?).await?;
            if old_size != item.source.size {
                return Err(error("源对象大小发生变化"));
            }
            let (stream, size, expected_plain_hash) = match &item.encoding {
                Encoding::Ctr { nonce } => {
                    let (_, expected) =
                        digest(plain_stream(storage, &item.source.key, old, &item.encoding).await?)
                            .await?;
                    let plain =
                        plain_stream(storage, &item.source.key, old, &item.encoding).await?;
                    // CTR 加解密对称。新盐保证是全新的密钥，同一 nonce 在不同密钥下复用安全，
                    // 因而无需同时改写大附件的 IV 与 Manifest。不得用于相同密钥的原地重加密。
                    let encrypted = new.decrypt_streaming(plain, nonce, 0).map_err(error)?;
                    (encrypted, item.source.size, expected)
                }
                _ => {
                    let mut plaintext = Zeroizing::new(
                        old.decrypt(&read_limited(storage, &item.source.key, MAX_GCM_BYTES).await?)
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
                    (byte_stream(encrypted), size, expected)
                }
            };
            storage.write(&stage_key, size, stream).await?;
            notify(progress(checkpoint, "verifying"));
            let (_, actual_plain_hash) =
                digest(plain_stream(storage, &stage_key, new, &item.encoding).await?).await?;
            let (actual_size, new_cipher_hash) = digest(storage.read(&stage_key).await?).await?;
            if actual_plain_hash != expected_plain_hash || actual_size != size {
                return Err(error("新密文读回校验失败，未替换原对象"));
            }
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
            Some(_) => Some(digest(storage.read(&item.source.key).await?).await?),
            None => None, // 本地替换数据/ETag 的中断窗口，由已验证的新密文修复。
        };
        let already_committed =
            current.as_ref() == Some(&(staged.size, staged.new_cipher_hash.clone()));
        if !already_committed
            && current.as_ref().is_some_and(|(size, hash)| {
                *size != item.source.size || hash != &staged.old_cipher_hash
            })
        {
            return Err(error(format!(
                "目标对象变化，拒绝覆盖: {}",
                item.source.key
            )));
        }
        if !already_committed {
            if digest(storage.read(&stage_key).await?).await?
                != (staged.size, staged.new_cipher_hash.clone())
            {
                return Err(error("临时密文已损坏，停止替换数据"));
            }
            // 原密文备份在替换前读回校验；验证目标与持久保存检查点后才删除。
            if storage.head(&backup_key).await?.is_none() {
                if current.is_none() {
                    return Err(error("原对象及旧密文备份均缺失，停止以免丢失数据"));
                }
                storage
                    .copy(&item.source.key, &backup_key, item.source.size)
                    .await?;
            }
            if digest(storage.read(&backup_key).await?).await?
                != (item.source.size, staged.old_cipher_hash.clone())
            {
                return Err(error("旧密文备份校验失败，未替换数据"));
            }
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
            storage
                .copy(&stage_key, &item.source.key, staged.size)
                .await?
        };
        if digest(storage.read(&item.source.key).await?).await?
            != (staged.size, staged.new_cipher_hash.clone())
        {
            return Err(error("目标对象校验失败，保留临时密文供恢复"));
        }
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
