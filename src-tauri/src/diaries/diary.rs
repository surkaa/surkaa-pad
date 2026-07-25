use crate::cryptos::Crypto;
use crate::state::AppState;

use crate::attachments::AttachmentMeta;
use crate::caches::DiaryMemoryCache;
use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
use crate::diaries::diary_migration::{migrate_manifest_bytes, MigrationContext, CURRENT_VERSION};
use crate::diaries::diary_store::DiaryStore;
use crate::diaries::diary_types::DiarySummary;
use crate::diaries::{DiaryContent, DiaryError, DiaryManifest};
use crate::utils::id_generate::generate_descending_id;
use chrono::Utc;
use dashmap::DashMap;
use serde_json::from_slice;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

/// 每个日记的 manifest 更新互斥锁，防止并发 read-modify-write 导致附件丢失
static MANIFEST_LOCKS: LazyLock<DashMap<String, Arc<Mutex<()>>>> = LazyLock::new(DashMap::new);

pub async fn save_diary<C: Into<DiaryContent>>(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    content: C,
) -> Result<(DiarySummary, DiaryContent), DiaryError> {
    let content = content.into();
    let id = generate_descending_id();
    // 创建一个简单的 manifest
    let manifest = DiaryManifest {
        id: id.clone(),
        algorithm: Gcm,
        content: content.clone(),
        created: Utc::now().timestamp_millis(),
        updated: Utc::now().timestamp_millis(),
        attachments: Vec::new(),
        version: CURRENT_VERSION,
    };

    // 序列化为 JSON
    let manifest_json = serde_json::to_vec(&manifest)?;

    // 加密 manifest
    let encrypted_manifest = crypto.encrypt(&manifest_json)?;

    // 上传到存储（LocalStore 写入 LFC，RemoteStore 写入 OSS + LFC 写透）
    let etag = store.upload_manifest(&id, &encrypted_manifest).await?;

    // 保存到内存缓存中
    cache.insert(&id, manifest.clone(), etag);

    Ok((DiarySummary::from_manifest(manifest), content))
}

/// 获取并解密指定 ID 的日记 manifest 自动处理缓存问题
pub async fn get_diary(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    id: &str,
) -> Result<DiaryManifest, DiaryError> {
    if id.is_empty() {
        return Err(DiaryError::EmptyId);
    }

    // 获取远程 etag 用于缓存校验
    let remote_etag = store.get_manifest_etag(id).await?;

    // 检查内存缓存
    if let Some((diary, etag)) = cache.get(id) {
        if remote_etag.as_deref() == Some(&etag) {
            return Ok(diary.clone());
        }
    }

    // 从存储下载（RemoteStore 会检查本地文件缓存，LocalStore 直接读取）
    let (encrypted_data, etag) = store.download_manifest(id).await?;

    let manifest_bytes = crypto.decrypt(&encrypted_data)?;

    // 迁移步骤可以先幂等迁移附件对象；只有全部成功后才发布新版 manifest。
    let migration_context = MigrationContext {
        diary_id: id,
        store,
    };
    if let Some(new_bytes) = migrate_manifest_bytes(&migration_context, &manifest_bytes).await? {
        let re_encrypted = crypto.encrypt(&new_bytes)?;
        let new_etag = store.upload_manifest(id, &re_encrypted).await?;
        let manifest: DiaryManifest = from_slice(&new_bytes)?;
        cache.insert(id, manifest.clone(), new_etag);
        return Ok(manifest);
    }

    // 反序列化 JSON
    let manifest: DiaryManifest = from_slice(&manifest_bytes)?;

    // 更新内存缓存
    cache.insert(id, manifest.clone(), etag);

    Ok(manifest)
}

pub async fn delete_diary(
    cache: &DiaryMemoryCache,
    store: &dyn DiaryStore,
    id: &str,
) -> Result<(), DiaryError> {
    store.delete_diary_all(id).await?;

    // 删除内存缓存
    cache.remove(id);

    Ok(())
}

async fn update_diary(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    diary: &DiaryManifest,
) -> Result<(), DiaryError> {
    // 序列化为 JSON
    let manifest_json = serde_json::to_vec(diary)?;

    // 加密 manifest
    let encrypted_manifest = crypto.encrypt(&manifest_json)?;

    // 上传到存储，覆盖原有的 manifest
    let etag = store
        .upload_manifest(&diary.id, &encrypted_manifest)
        .await?;

    // 更新缓存
    cache.insert(&diary.id, diary.clone(), etag);

    Ok(())
}

pub async fn update_diary_content_only<C: Into<DiaryContent>>(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    id: &str,
    new_content: C,
) -> Result<DiarySummary, DiaryError> {
    // 先获取现有的 manifest
    let mut manifest = get_diary(cache, crypto, store, id).await?;

    // 更新内容和更新时间
    manifest.content = new_content.into();
    manifest.updated = Utc::now().timestamp_millis();

    update_diary(cache, crypto, store, &manifest).await?;

    Ok(DiarySummary::from_manifest(manifest))
}

pub async fn update_diary_attachment(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    id: &str,
    new_attachment: AttachmentMeta,
) -> Result<(), DiaryError> {
    // 防止并发 read-modify-write 导致附件丢失
    let lock = MANIFEST_LOCKS
        .entry(id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _guard = lock.lock().await;

    let mut diary = get_diary(cache, crypto, store, id).await?;
    // ID 是附件身份；filename 仅用于展示，可以被重命名。
    if let Some(existing) = diary
        .attachments
        .iter_mut()
        .find(|att| att.id == new_attachment.id)
    {
        *existing = new_attachment;
    } else {
        diary.attachments.push(new_attachment);
    }
    diary.updated = Utc::now().timestamp_millis();
    update_diary(cache, crypto, store, &diary).await?;
    Ok(())
}

pub async fn update_diary_attachment_filename(
    state: &AppState,
    id: &str,
    attachment_id: String,
    new_filename: String,
) -> Result<(), DiaryError> {
    let lock = MANIFEST_LOCKS
        .entry(id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _guard = lock.lock().await;

    let cache = state.diary_cache();
    let crypto = state.crypto();
    let store = state.diary_store();
    let mut diary = get_diary(&cache, &crypto, &*store, id).await?;
    if let Some(att) = diary
        .attachments
        .iter_mut()
        .find(|att| att.id == attachment_id)
    {
        att.filename = new_filename;
        diary.updated = Utc::now().timestamp_millis();
        update_diary(&cache, &crypto, &*store, &diary).await?;
        Ok(())
    } else {
        Err(DiaryError::AttachmentNotFound(attachment_id))
    }
}

pub async fn delete_diary_attachment(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    store: &dyn DiaryStore,
    id: &str,
    attachment_id: &str,
) -> Result<(), DiaryError> {
    let lock = MANIFEST_LOCKS
        .entry(id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _guard = lock.lock().await;

    let mut diary = get_diary(cache, crypto, store, id).await?;
    diary.attachments.retain(|att| att.id != attachment_id);
    diary.content.remove_attachment(attachment_id);
    diary.updated = Utc::now().timestamp_millis();
    update_diary(cache, crypto, store, &diary).await?;
    Ok(())
}
