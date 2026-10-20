use super::storage::{read_limited, RekeyStorage};
use super::types::{Encoding, Entry, WorkItem};
use super::{error, Result};
use crate::cryptos::{crypto_types::EncryptionAlgorithm, Crypto};
use crate::diaries::DiaryManifest;
use crate::object_locations::{ObjectLocations, StoredObject};
use std::collections::{BTreeMap, HashSet};
use zeroize::Zeroizing;

// 现有 GCM 是整对象认证格式，不能擅自当 CTR 流处理。过大时预检查拒绝，而不是改到一半才 OOM。
pub(super) const MAX_GCM_BYTES: usize = 64 * 1024 * 1024;

pub(super) async fn inventory(
    storage: &dyn RekeyStorage,
    old: &Crypto,
) -> Result<(Vec<Entry>, Vec<WorkItem>)> {
    let mut entries = storage.list().await?;
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    let mut attachment_encodings = BTreeMap::new();
    let mut required_attachments = HashSet::new();
    for entry in &entries {
        if let Some(StoredObject::DiaryManifest { diary_id }) = ObjectLocations::parse(&entry.key) {
            if entry.size > MAX_GCM_BYTES as u64 {
                return Err(error(format!(
                    "日记 {} 超过 64 MiB 的安全读取上限，尚未改密",
                    entry.key
                )));
            }
            let plaintext = Zeroizing::new(
                old.decrypt(&read_limited(storage, &entry.key, MAX_GCM_BYTES).await?)
                    .map_err(error)?,
            );
            let manifest: DiaryManifest =
                crate::diaries::deserialize_current_manifest(&diary_id, &plaintext)
                    .map_err(error)?;
            for attachment in manifest.attachments {
                let key = ObjectLocations::diary_attachment(&diary_id, &attachment.id);
                if !required_attachments.insert(key.clone()) {
                    return Err(error("日记中存在重复附件 ID"));
                }
                if attachment.encrypted {
                    let encoding = match attachment.algorithm {
                        EncryptionAlgorithm::Gcm => Encoding::Gcm,
                        EncryptionAlgorithm::Ctr if attachment.nonce.len() == 16 => Encoding::Ctr {
                            nonce: attachment.nonce,
                        },
                        _ => return Err(error(format!("附件 {key} 的加密参数无效"))),
                    };
                    attachment_encodings.insert(key, encoding);
                }
            }
        }
    }
    let actual_keys: HashSet<_> = entries.iter().map(|e| e.key.clone()).collect();
    for key in &required_attachments {
        if !actual_keys.contains(key) {
            return Err(error(format!("日记引用的附件缺失: {key}，尚未改密")));
        }
    }
    let mut work = vec![];
    for entry in &entries {
        let encoding = match ObjectLocations::parse(&entry.key) {
            Some(StoredObject::DiaryManifest { .. }) => Some(Encoding::Manifest),
            Some(
                StoredObject::AiSessionMeta { .. }
                | StoredObject::AiSessionMessageBlock { .. }
                | StoredObject::SyncedSettings,
            ) => Some(Encoding::Gcm),
            Some(StoredObject::DiaryAttachment { .. }) => {
                if !required_attachments.contains(&entry.key) {
                    return Err(error(format!(
                        "存在未被日记引用的附件: {}，请先处理",
                        entry.key
                    )));
                }
                attachment_encodings.remove(&entry.key)
            }
            Some(StoredObject::DiaryAttachmentBackup { .. }) => {
                return Err(error("存在未完成的附件事务，请先恢复附件操作"))
            }
            Some(StoredObject::VaultBootstrap) => None,
            None if ObjectLocations::diary_edit_lease_id(&entry.key).is_some() => None,
            None if entry.key.ends_with('/') && entry.size == 0 => None,
            None => {
                return Err(error(format!(
                    "无法识别的对象: {}，不会冒险遗漏它进行改密",
                    entry.key
                )))
            }
        };
        if let Some(encoding) = encoding {
            if !matches!(encoding, Encoding::Ctr { .. }) {
                if entry.size > MAX_GCM_BYTES as u64 {
                    return Err(error(format!(
                        "认证加密对象 {} 超过 64 MiB 的安全读取上限，尚未改密",
                        entry.key
                    )));
                }
                let _validated_plaintext = Zeroizing::new(
                    old.decrypt(&read_limited(storage, &entry.key, MAX_GCM_BYTES).await?)
                        .map_err(error)?,
                );
            }
            work.push(WorkItem {
                source: entry.clone(),
                encoding,
            });
        }
    }
    // 修改附件 ETag 后，最后重加密 Manifest；暂停期间不允许读到混合状态。
    work.sort_by_key(|item| matches!(item.encoding, Encoding::Manifest));
    Ok((entries, work))
}
