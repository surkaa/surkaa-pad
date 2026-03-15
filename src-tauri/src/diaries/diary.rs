use crate::cryptos::Crypto;
use crate::object::OssClient;
use crate::storages::remote_manifest_key;

use crate::attachments::AttachmentMeta;
use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
use crate::diaries::diary_types::DiarySummary;
use crate::diaries::DiaryManifest;
use crate::utils::id_generate::generate_descending_id;
use chrono::Utc;
use serde_json::from_slice;
use crate::caches::DiaryMemoryCache;

pub async fn save_diary(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    content: &str,
) -> Result<(DiarySummary, String), String> {
    let id = generate_descending_id();
    // 创建一个简单的 manifest
    let manifest = DiaryManifest {
        id: id.clone(),
        algorithm: Gcm,
        content: content.to_string(),
        created: Utc::now().timestamp_millis(),
        updated: Utc::now().timestamp_millis(),
        attachments: Vec::new(),
    };

    // 序列化为 JSON
    let manifest_json = serde_json::to_vec(&manifest)
        .map_err(|e| format!("Failed to serialize manifest: {}", e))?;

    // 加密 manifest
    let encrypted_manifest = crypto.encrypt(&manifest_json)?;

    // 上传到 OSS
    let object_key = remote_manifest_key(&id);
    let etag = client
        .upload_bytes(&object_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload manifest: {}", e))?;

    // 保存到内存缓存中
    cache.insert(&id, manifest.clone(), etag);

    Ok((DiarySummary::from_manifest(manifest), content.to_string()))
}

/// 获取并解密指定 ID 的日记 manifest
pub async fn get_diary(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
) -> Result<DiaryManifest, String> {
    if id.is_empty() {
        return Err("ID不能为空".to_string());
    }
    let object_key = remote_manifest_key(id);
    let metadata = client.get_metadata(&object_key).await?;
    if let Some((diary, etag)) = cache.get(id) {
        if etag == metadata.etag() {
            return Ok(diary.clone());
        }
    }
    let encrypted_data = client
        .download_bytes(&object_key)
        .await
        .map_err(|e| format!("未能下载加密清单用于缓存: {}", e))?;

    let manifest_bytes = crypto.decrypt(&encrypted_data)?;

    // 反序列化 JSON
    let manifest: DiaryManifest =
        from_slice(&manifest_bytes).map_err(|e| format!("未能解析manifest:{}", e))?;

    // 更新缓存
    cache.insert(id, manifest.clone(), metadata.etag().to_string());

    Ok(manifest)
}

pub async fn delete_diary(
    cache: &DiaryMemoryCache,
    client: &OssClient,
    id: &str,
) -> Result<(), String> {
    client.delete_with_prefix(id).await?;

    // 删除缓存
    cache.remove(id);

    Ok(())
}

async fn update_diary(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    diary: &DiaryManifest,
) -> Result<(), String> {
    // 序列化为 JSON
    let manifest_json = serde_json::to_vec(&diary).map_err(|e| format!("未能序列化日记: {}", e))?;

    // 加密 manifest
    let encrypted_manifest = crypto.encrypt(&manifest_json)?;

    // 上传到 OSS，覆盖原有的 manifest
    let object_key = remote_manifest_key(&diary.id);
    let etag = client
        .upload_bytes(&object_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

    // 更新缓存
    cache.insert(&diary.id, diary.clone(), etag);

    Ok(())
}

pub async fn update_diary_content_only(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    new_content: &str,
) -> Result<DiarySummary, String> {
    // 先获取现有的 manifest
    let mut manifest = get_diary(cache, crypto, client, id).await?;

    // 更新内容和更新时间
    manifest.content = new_content.to_string();
    manifest.updated = Utc::now().timestamp_millis();

    update_diary(cache, crypto, client, &manifest).await?;

    Ok(DiarySummary::from_manifest(manifest))
}

pub async fn update_diary_attachment(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    new_attachment: AttachmentMeta,
) -> Result<(), String> {
    let mut diary = get_diary(cache, crypto, client, id).await?;
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
    update_diary(cache, crypto, client, &diary).await?;
    Ok(())
}

pub async fn delete_diary_attachment(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    filename: &str,
) -> Result<(), String> {
    let mut diary = get_diary(cache, crypto, client, id).await?;
    diary.attachments.retain(|att| att.filename != filename);
    diary.updated = Utc::now().timestamp_millis();
    update_diary(cache, crypto, client, &diary).await?;
    Ok(())
}
