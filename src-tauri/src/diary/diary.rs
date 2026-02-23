use crate::crypto::Crypto;
use crate::object::OssClient;
use crate::storage::remote_manifest_key;

use crate::attachment::AttachmentMeta;
use crate::crypto::types::EncryptionAlgorithm::Gcm;
use crate::diary::{DiaryManifest, DiaryMemoryCache};
use chrono::Utc;
use serde_json::from_slice;
use crate::diary::types::DiarySummary;
use crate::utils::id_generate::generate_descending_id;

// TODO 统一函数命名格式 以及diary和manifest
pub async fn save_diary(
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
    let (ciphertext, nonce) = crypto.encrypt(&manifest_json)?;

    // 组合 nonce 和 ciphertext，前面放 nonce
    let mut encrypted_manifest = nonce;
    encrypted_manifest.extend_from_slice(&ciphertext);

    // 上传到 OSS
    let object_key = remote_manifest_key(&id);
    client
        .upload_bytes(&object_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload manifest: {}", e))?;

    Ok((DiarySummary::from_manifest(manifest), content.to_string()))
}

/// 获取并解密指定 ID 的日记 manifest
pub async fn diary_get(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
) -> Result<DiaryManifest, String> {
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

    let manifest_bytes = crypto.decrypt_from_full_ciphertext(&encrypted_data)?;

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
    let (ciphertext, nonce) = crypto.encrypt(&manifest_json)?;

    // 组合 nonce 和 ciphertext，前面放 nonce
    let mut encrypted_manifest = nonce;
    encrypted_manifest.extend_from_slice(&ciphertext);

    // 上传到 OSS，覆盖原有的 manifest
    let object_key = remote_manifest_key(&diary.id);
    client
        .upload_bytes(&object_key, &encrypted_manifest)
        .await
        .map_err(|e| format!("Failed to upload updated manifest: {}", e))?;

    // 获取ETag并更新缓存
    let metadata = client.get_metadata(&object_key).await?;
    cache.insert(&diary.id, diary.clone(), metadata.etag().to_string());

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
    let mut manifest = diary_get(cache, crypto, client, id).await?;

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
    new_attachment: &AttachmentMeta,
) -> Result<(), String> {
    let mut diary = diary_get(&cache, &crypto, &client, id).await?;
    diary.attachments.push(new_attachment.clone());
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
    let mut diary = diary_get(&cache, &crypto, &client, id).await?;
    diary.attachments.retain(|att| att.filename != filename);
    diary.updated = Utc::now().timestamp_millis();
    update_diary(cache, crypto, client, &diary).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::time::Duration;

    #[serial]
    #[tokio::test]
    async fn test_diary_crud_lifecycle() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let cache = DiaryMemoryCache::new();

        // 判断为空，确保测试环境干净
        let (objects, _) = client.list("", None, None).await.expect("未能列出对象");
        assert!(
            objects.is_empty(),
            "测试环境不干净。请确保运行测试前OSS桶是空的。"
        );

        // 测试创建
        let initial_content = "Integration test diary content.";
        let (summary, content) = save_diary(&crypto, &client, initial_content)
            .await
            .expect("未能保存日记");

        assert_eq!(content, initial_content);
        assert!(!summary.id.is_empty());
        let id = summary.id.clone();

        // 测试读取 - 验证远端拉取并写入缓存
        let fetched_manifest = diary_get(&cache, &crypto, &client, &id)
            .await
            .expect("远程获取日记失败");

        assert_eq!(fetched_manifest.id, id);
        assert_eq!(fetched_manifest.content, initial_content);

        // 为了确保 update 生成的时间戳严格大于前一次，休眠防 Flaky Test
        tokio::time::sleep(Duration::from_millis(5)).await;

        // 测试更新
        let updated_content = "Updated content for testing.";
        let updated_summary =
            update_diary_content_only(&cache, &crypto, &client, &id, updated_content)
                .await
                .expect("未能更新日记");

        assert!(updated_summary.updated > summary.updated);

        // 测试再次读取 - 验证缓存失效/更新机制
        let refetched_manifest = diary_get(&cache, &crypto, &client, &id)
            .await
            .expect("未能重新获取更新的日记");

        assert_eq!(refetched_manifest.content, updated_content);

        // 测试删除
        delete_diary(&cache, &client, &id)
            .await
            .expect("删除日记失败");

        // 验证删除有效性
        let not_found_result = diary_get(&cache, &crypto, &client, &id).await;
        assert!(not_found_result.is_err(), "删除后日记不应被检索");
    }
}
