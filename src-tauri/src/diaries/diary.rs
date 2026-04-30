use crate::cryptos::Crypto;
use crate::object::OssClient;
use crate::state::AppState;
use crate::storages::remote_manifest_key;

use crate::attachments::AttachmentMeta;
use crate::caches::{DiaryMemoryCache, LocalFileCache};
use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
use crate::diaries::diary_types::DiarySummary;
use crate::diaries::diary_migration::{migrate_manifest_bytes, CURRENT_VERSION};
use crate::diaries::{DiaryError, DiaryManifest};
use crate::utils::id_generate::generate_descending_id;
use chrono::Utc;
use serde_json::from_slice;

pub async fn save_diary(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    content: &str,
) -> Result<(DiarySummary, String), DiaryError> {
    let id = generate_descending_id();
    // 创建一个简单的 manifest
    let manifest = DiaryManifest {
        id: id.clone(),
        algorithm: Gcm,
        content: content.to_string(),
        created: Utc::now().timestamp_millis(),
        updated: Utc::now().timestamp_millis(),
        attachments: Vec::new(),
        version: CURRENT_VERSION,
    };

    // 序列化为 JSON
    let manifest_json = serde_json::to_vec(&manifest)?;

    // 加密 manifest
    let encrypted_manifest = crypto.encrypt(&manifest_json)?;

    // 上传到 OSS
    let object_key = remote_manifest_key(&id);
    let etag = client
        .upload_bytes(&object_key, &encrypted_manifest)
        .await?;

    // 保存到内存缓存中
    cache.insert(&id, manifest.clone(), etag);
    // 保存到本地文件缓存中
    lfc.save_bytes(&object_key, &encrypted_manifest).await?;

    Ok((DiarySummary::from_manifest(manifest), content.to_string()))
}

/// 获取并解密指定 ID 的日记 manifest 自动处理缓存问题
pub async fn get_diary(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
) -> Result<DiaryManifest, DiaryError> {
    if id.is_empty() {
        return Err(DiaryError::EmptyId);
    }
    let object_key = remote_manifest_key(id);
    let metadata = client.get_metadata(&object_key).await?;

    // 检查内存缓存
    if let Some((diary, etag)) = cache.get(id) {
        if metadata.etag.as_deref() == Some(&etag) {
            return Ok(diary.clone());
        }
    }

    // 检查文件缓存
    if let Some(etag) = lfc.get(&object_key).await? {
        if metadata.etag.as_deref() == Some(&etag) {
            let cache_bytes = lfc.get_data(&object_key).await?;
            let manifest_bytes = crypto.decrypt(&cache_bytes)?;
            // 迁移钩子：JSON 层面版本升级
            if let (true, Some(new_bytes)) = migrate_manifest_bytes(&manifest_bytes)? {
                let re_encrypted = crypto.encrypt(&new_bytes)?;
                let new_etag = client.upload_bytes(&object_key, &re_encrypted).await?;
                lfc.save_bytes(&object_key, &re_encrypted).await?;
                let manifest: DiaryManifest = from_slice(&new_bytes)?;
                cache.insert(id, manifest.clone(), new_etag);
                return Ok(manifest);
            }
            // 反序列化 JSON
            let manifest: DiaryManifest = from_slice(&manifest_bytes)?;
            // 更新缓存
            cache.insert(id, manifest.clone(), metadata.etag.clone().unwrap_or_default());
            return Ok(manifest);
        } else {
            lfc.delete(&object_key).await;
        }
    }

    let encrypted_data = client
        .download_bytes(&object_key)
        .await?;

    let manifest_bytes = crypto.decrypt(&encrypted_data)?;

    // 迁移钩子：JSON 层面版本升级
    if let (true, Some(new_bytes)) = migrate_manifest_bytes(&manifest_bytes)? {
        let re_encrypted = crypto.encrypt(&new_bytes)?;
        let new_etag = client.upload_bytes(&object_key, &re_encrypted).await?;
        lfc.save_bytes(&object_key, &re_encrypted).await?;
        let manifest: DiaryManifest = from_slice(&new_bytes)?;
        cache.insert(id, manifest.clone(), new_etag);
        return Ok(manifest);
    }

    // 反序列化 JSON
    let manifest: DiaryManifest = from_slice(&manifest_bytes)?;

    // 更新内存缓存
    cache.insert(id, manifest.clone(), metadata.etag.clone().unwrap_or_default());
    // 更新本地文件缓存
    lfc.save_bytes(&object_key, &encrypted_data).await?;

    Ok(manifest)
}

pub async fn delete_diary(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    client: &OssClient,
    id: &str,
) -> Result<(), DiaryError> {
    client.delete_with_prefix(id).await?;

    // 删除缓存
    cache.remove(id);
    // 删除本地缓存
    let key = remote_manifest_key(id);
    lfc.delete(&key).await;

    Ok(())
}

async fn update_diary(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    diary: &DiaryManifest,
) -> Result<(), DiaryError> {
    // 序列化为 JSON
    let manifest_json = serde_json::to_vec(&diary)?;

    // 加密 manifest
    let encrypted_manifest = crypto.encrypt(&manifest_json)?;

    // 上传到 OSS，覆盖原有的 manifest
    let object_key = remote_manifest_key(&diary.id);
    let etag = client
        .upload_bytes(&object_key, &encrypted_manifest)
        .await?;

    // 更新缓存
    cache.insert(&diary.id, diary.clone(), etag);
    // 更新本地缓存 会自动替换掉旧的
    lfc.save_bytes(&object_key, &encrypted_manifest).await?;

    Ok(())
}

pub async fn update_diary_content_only(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    new_content: &str,
) -> Result<DiarySummary, DiaryError> {
    // 先获取现有的 manifest
    let mut manifest = get_diary(cache, lfc, crypto, client, id).await?;

    // 更新内容和更新时间
    manifest.content = new_content.to_string();
    manifest.updated = Utc::now().timestamp_millis();

    update_diary(cache, lfc, crypto, client, &manifest).await?;

    Ok(DiarySummary::from_manifest(manifest))
}

pub async fn update_diary_attachment(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    new_attachment: AttachmentMeta,
) -> Result<(), DiaryError> {
    let mut diary = get_diary(cache, lfc, crypto, client, id).await?;
    // 判断是否已存在同名附件，若存在则替换，否则添加
    if let Some(existing) = diary
        .attachments
        .iter_mut()
        .find(|att| att.filename == new_attachment.filename)
    {
        *existing = new_attachment;
    } else {
        diary.attachments.push(new_attachment);
    }
    diary.updated = Utc::now().timestamp_millis();
    update_diary(cache, lfc, crypto, client, &diary).await?;
    Ok(())
}

pub async fn update_diary_attachment_filename(
    state: &AppState,
    id: &str,
    old_filename: String,
    new_filename: String,
    new_content: String,
) -> Result<(), DiaryError> {
    let cache = &state.diary_cache();
    let lfc = &state.local_file_cache();
    let crypto = &state.crypto();
    let client = &state.oss_client();
    let mut diary = get_diary(cache, lfc, crypto, client, id).await?;
    if let Some(att) = diary
        .attachments
        .iter_mut()
        .find(|att| att.filename == old_filename)
    {
        att.filename = new_filename;
        // 更新 diary.content
        diary.content = new_content;
        diary.updated = Utc::now().timestamp_millis();
        update_diary(cache, lfc, crypto, client, &diary).await?;
        Ok(())
    } else {
        Err(DiaryError::AttachmentNotFound(old_filename))
    }
}

pub async fn delete_diary_attachment(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    filename: &str,
) -> Result<(), DiaryError> {
    let mut diary = get_diary(cache, lfc, crypto, client, id).await?;
    diary.attachments.retain(|att| att.filename != filename);
    diary.updated = Utc::now().timestamp_millis();
    update_diary(cache, lfc, crypto, client, &diary).await?;
    Ok(())
}
