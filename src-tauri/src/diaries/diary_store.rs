use async_trait::async_trait;
use chrono::Utc;

use crate::attachments::AttachmentMeta;
use crate::caches::LocalFileCache;
use crate::diaries::DiaryError;
use crate::object::{NextToken, OssClient};
use crate::storages::{diary_id_from_manifest_key, remote_attachments_key, remote_manifest_key};
use crate::stream::ByteStream;

/// 日记存储抽象，隔离本地与远程的 I/O 差异
#[async_trait]
pub trait DiaryStore: Send + Sync {
    /// 上传/保存加密的 manifest 数据，返回 etag
    async fn upload_manifest(&self, id: &str, data: &[u8]) -> Result<String, DiaryError>;
    /// 下载/读取加密的 manifest 数据，返回 (数据, etag)
    async fn download_manifest(&self, id: &str) -> Result<(Vec<u8>, String), DiaryError>;
    /// 获取 manifest 的元数据（etag），用于缓存校验。不存在时返回 Ok(None)
    async fn get_manifest_etag(&self, id: &str) -> Result<Option<String>, DiaryError>;
    /// 删除日记（manifest + 所有附件）
    async fn delete_diary_all(&self, id: &str) -> Result<(), DiaryError>;
    /// 列出日记 ID（分页）
    async fn list_diary_ids(
        &self,
        next_token: NextToken,
    ) -> Result<(Vec<String>, NextToken), DiaryError>;
    /// 上传附件（流式），返回 etag
    async fn upload_attachment(
        &self,
        id: &str,
        filename: &str,
        size: u64,
        mimetype: &str,
        stream: ByteStream,
    ) -> Result<String, DiaryError>;
    /// 获取附件流，支持 Range 请求
    async fn download_attachment(
        &self,
        id: &str,
        filename: &str,
        range: Option<(u64, u64)>,
        known_etag: Option<&str>,
    ) -> Result<(ByteStream, u64), DiaryError>;
    /// 删除附件
    async fn delete_attachment(&self, id: &str, filename: &str) -> Result<(), DiaryError>;
    /// 获取附件的完整 URL（用于前端展示）
    async fn get_attachment_url(
        &self,
        id: &str,
        attachment: &AttachmentMeta,
    ) -> Result<String, DiaryError>;
    /// 重命名附件
    async fn rename_attachment(
        &self,
        id: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), DiaryError>;
    /// 初始化分片上传，返回 upload_id
    async fn initiate_multipart_upload(
        &self,
        key: &str,
        content_type: &str,
    ) -> Result<String, DiaryError>;
    /// 上传单个分片，返回 (etag, part_number)
    async fn upload_part(
        &self,
        key: &str,
        part_number: u32,
        upload_id: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(String, u32), DiaryError>;
    /// 完成分片上传，返回 composite ETag
    async fn complete_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
        parts: Vec<(String, u32)>,
    ) -> Result<String, DiaryError>;
    /// 取消分片上传
    async fn abort_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
    ) -> Result<(), DiaryError>;
}

// =============================================================================
// LocalStore — 仅使用 LocalFileCache
// =============================================================================

pub struct LocalStore {
    lfc: LocalFileCache,
}

impl LocalStore {
    pub fn new(lfc: LocalFileCache) -> Self {
        Self { lfc }
    }
}

#[async_trait]
impl DiaryStore for LocalStore {
    async fn upload_manifest(&self, id: &str, data: &[u8]) -> Result<String, DiaryError> {
        let key = remote_manifest_key(id);
        self.lfc.save_bytes(&key, data).await?;
        let etag = format!("{:X}", md5::compute(data));
        Ok(etag)
    }

    async fn download_manifest(&self, id: &str) -> Result<(Vec<u8>, String), DiaryError> {
        let key = remote_manifest_key(id);
        let etag = self
            .lfc
            .get(&key)
            .await?
            .ok_or(crate::caches::CacheError::NotFound)?;
        let data = self.lfc.get_data(&key).await?;
        Ok((data, etag))
    }

    async fn get_manifest_etag(&self, id: &str) -> Result<Option<String>, DiaryError> {
        let key = remote_manifest_key(id);
        Ok(self.lfc.get(&key).await?)
    }

    async fn delete_diary_all(&self, id: &str) -> Result<(), DiaryError> {
        let prefix = format!("{}/", id);
        if let Ok(all) = self.lfc.get_all().await {
            for (key, _) in all {
                if key.starts_with(&prefix) || key == id {
                    self.lfc.delete(&key).await;
                }
            }
        }
        Ok(())
    }

    async fn list_diary_ids(
        &self,
        next_token: NextToken,
    ) -> Result<(Vec<String>, NextToken), DiaryError> {
        let all = self.lfc.get_all().await?;
        let mut ids: Vec<String> = all
            .into_iter()
            .filter_map(|(key, _)| diary_id_from_manifest_key(&key))
            .collect();
        ids.sort_by(|a, b| b.cmp(a));

        // 简单分页：next_token 编码为偏移量
        let offset: usize = next_token
            .and_then(|t| t.parse().ok())
            .unwrap_or(0);
        let page_size = 50;
        let end = (offset + page_size).min(ids.len());
        let page = ids[offset..end].to_vec();
        let next = if end < ids.len() {
            Some(end.to_string())
        } else {
            None
        };
        Ok((page, next))
    }

    async fn upload_attachment(
        &self,
        id: &str,
        filename: &str,
        _size: u64,
        _mimetype: &str,
        stream: ByteStream,
    ) -> Result<String, DiaryError> {
        let key = remote_attachments_key(id, filename);
        let data = crate::stream::collect_data(stream)
            .await
            .map_err(|e| DiaryError::Object(crate::object::ObjectError::OperationFailed(e.to_string())))?;
        self.lfc.save_bytes(&key, &data).await?;
        let etag = format!("{:X}", md5::compute(&data));
        Ok(etag)
    }

    async fn download_attachment(
        &self,
        id: &str,
        filename: &str,
        range: Option<(u64, u64)>,
        _known_etag: Option<&str>,
    ) -> Result<(ByteStream, u64), DiaryError> {
        let key = remote_attachments_key(id, filename);
        let stream = self.lfc.get_stream(&key, range).await?;
        Ok((stream, 0))
    }

    async fn delete_attachment(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        self.lfc.delete(&key).await;
        Ok(())
    }

    async fn get_attachment_url(
        &self,
        id: &str,
        attachment: &AttachmentMeta,
    ) -> Result<String, DiaryError> {
        let encoded_filename = urlencoding::encode(&attachment.filename);
        let timestamp = Utc::now().timestamp();
        Ok(format!(
            "http://attachment.localhost/{}/{}?t={}",
            id, encoded_filename, timestamp
        ))
    }

    async fn rename_attachment(
        &self,
        id: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), DiaryError> {
        let old_key = remote_attachments_key(id, old_name);
        let new_key = remote_attachments_key(id, new_name);
        // 读取旧数据 → 写入新 key → 删除旧 key
        let data = self.lfc.get_data(&old_key).await?;
        self.lfc.save_bytes(&new_key, &data).await?;
        self.lfc.delete(&old_key).await;
        Ok(())
    }

    async fn initiate_multipart_upload(
        &self,
        _key: &str,
        _content_type: &str,
    ) -> Result<String, DiaryError> {
        // 本地模式不需要 multipart upload，返回空 upload_id
        Ok(String::new())
    }

    async fn upload_part(
        &self,
        _key: &str,
        _part_number: u32,
        _upload_id: &str,
        _data: Vec<u8>,
        _content_type: &str,
    ) -> Result<(String, u32), DiaryError> {
        // 本地模式下分片由 ChunkedSaveHandle 直接写入，此方法不应被调用
        Err(DiaryError::Object(crate::object::ObjectError::OperationFailed(
            "LocalStore does not support upload_part".into(),
        )))
    }

    async fn complete_multipart_upload(
        &self,
        _key: &str,
        _upload_id: &str,
        _parts: Vec<(String, u32)>,
    ) -> Result<String, DiaryError> {
        // 本地模式下由 ChunkedSaveHandle::finalize 处理
        Err(DiaryError::Object(crate::object::ObjectError::OperationFailed(
            "LocalStore does not support complete_multipart_upload".into(),
        )))
    }

    async fn abort_multipart_upload(
        &self,
        _key: &str,
        _upload_id: &str,
    ) -> Result<(), DiaryError> {
        // 本地模式下由 ChunkedSaveHandle::abort 处理
        Ok(())
    }
}

// =============================================================================
// RemoteStore — OSS + LFC 写透缓存
// =============================================================================

pub struct RemoteStore {
    lfc: LocalFileCache,
    client: OssClient,
}

impl RemoteStore {
    pub fn new(lfc: LocalFileCache, client: OssClient) -> Self {
        Self { lfc, client }
    }
}

#[async_trait]
impl DiaryStore for RemoteStore {
    async fn upload_manifest(&self, id: &str, data: &[u8]) -> Result<String, DiaryError> {
        let key = remote_manifest_key(id);
        let etag = self.client.upload_bytes(&key, data).await?;
        let _ = self.lfc.save_bytes(&key, data).await;
        Ok(etag)
    }

    async fn download_manifest(&self, id: &str) -> Result<(Vec<u8>, String), DiaryError> {
        let key = remote_manifest_key(id);
        // 优先检查本地缓存
        if let Some(cached_etag) = self.lfc.get(&key).await? {
            let metadata = self.client.get_metadata(&key).await?;
            if metadata.etag.as_deref() == Some(&cached_etag) {
                let data = self.lfc.get_data(&key).await?;
                return Ok((data, cached_etag));
            }
        }
        // 缓存未命中，从 OSS 下载
        let data = self.client.download_bytes(&key).await?;
        let metadata = self.client.get_metadata(&key).await?;
        let etag = metadata.etag.unwrap_or_default();
        let _ = self.lfc.save_bytes(&key, &data).await;
        Ok((data, etag))
    }

    async fn get_manifest_etag(&self, id: &str) -> Result<Option<String>, DiaryError> {
        let key = remote_manifest_key(id);
        let metadata = self.client.get_metadata(&key).await?;
        Ok(metadata.etag)
    }

    async fn delete_diary_all(&self, id: &str) -> Result<(), DiaryError> {
        self.client.delete_with_prefix(id).await?;
        // 清理本地缓存
        let prefix = format!("{}/", id);
        if let Ok(all) = self.lfc.get_all().await {
            for (key, _) in all {
                if key.starts_with(&prefix) || key == id {
                    self.lfc.delete(&key).await;
                }
            }
        }
        Ok(())
    }

    async fn list_diary_ids(
        &self,
        next_token: NextToken,
    ) -> Result<(Vec<String>, NextToken), DiaryError> {
        let (objects, nt) = self.client.list("", next_token).await?;
        let ids: Vec<String> = objects
            .into_iter()
            .filter_map(|obj| diary_id_from_manifest_key(&obj.key))
            .collect();
        Ok((ids, nt))
    }

    async fn upload_attachment(
        &self,
        id: &str,
        filename: &str,
        size: u64,
        mimetype: &str,
        stream: ByteStream,
    ) -> Result<String, DiaryError> {
        let key = remote_attachments_key(id, filename);
        // 包装流用于本地文件缓存
        let (wrapped_stream, handle) = self.lfc.save(&key, stream).await?;
        // 上传到 OSS
        match self.client.upload(&key, size, wrapped_stream, mimetype).await {
            Ok(etag) => {
                let _ = handle.finalize(&etag).await;
                Ok(etag)
            }
            Err(e) => {
                handle.abort().await;
                Err(DiaryError::Object(e))
            }
        }
    }

    async fn download_attachment(
        &self,
        id: &str,
        filename: &str,
        range: Option<(u64, u64)>,
        known_etag: Option<&str>,
    ) -> Result<(ByteStream, u64), DiaryError> {
        let key = remote_attachments_key(id, filename);
        if let Some(cached_etag) = self.lfc.get(&key).await? {
            // 已知 etag 匹配，直接使用缓存
            if known_etag.is_some_and(|k| k == cached_etag) {
                return Ok((self.lfc.get_stream(&key, range).await?, 0));
            }
            let metadata = self.client.get_metadata(&key).await?;
            if metadata.etag.as_deref() == Some(&cached_etag) {
                return Ok((
                    self.lfc.get_stream(&key, range).await?,
                    metadata.content_length.unwrap_or(0),
                ));
            } else {
                self.lfc.delete(&key).await;
            }
        }
        Ok(self.client.download(&key, range).await?)
    }

    async fn delete_attachment(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        self.client.delete(&key).await?;
        self.lfc.delete(&key).await;
        Ok(())
    }

    async fn get_attachment_url(
        &self,
        id: &str,
        attachment: &AttachmentMeta,
    ) -> Result<String, DiaryError> {
        let encoded_filename = urlencoding::encode(&attachment.filename);
        if attachment.encrypted {
            let timestamp = Utc::now().timestamp();
            Ok(format!(
                "http://attachment.localhost/{}/{}?t={}",
                id, encoded_filename, timestamp
            ))
        } else {
            let key = remote_attachments_key(id, &attachment.filename);
            let url = self.client.direct_url(&key).await?;
            Ok(url)
        }
    }

    async fn rename_attachment(
        &self,
        id: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), DiaryError> {
        let old_key = remote_attachments_key(id, old_name);
        let new_key = remote_attachments_key(id, new_name);
        self.client.rename(&old_key, &new_key).await?;
        Ok(())
    }

    async fn initiate_multipart_upload(
        &self,
        key: &str,
        content_type: &str,
    ) -> Result<String, DiaryError> {
        Ok(self.client.initiate_multipart_upload(key, content_type).await?)
    }

    async fn upload_part(
        &self,
        key: &str,
        part_number: u32,
        upload_id: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(String, u32), DiaryError> {
        Ok(self
            .client
            .upload_part(key, part_number, upload_id, data, content_type)
            .await?)
    }

    async fn complete_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
        parts: Vec<(String, u32)>,
    ) -> Result<String, DiaryError> {
        Ok(self
            .client
            .complete_multipart_upload(key, upload_id, parts)
            .await?)
    }

    async fn abort_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
    ) -> Result<(), DiaryError> {
        Ok(self.client.abort_multipart_upload(key, upload_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::DiaryMemoryCache;
    use crate::cryptos::Crypto;
    use crate::diaries::diary::{delete_diary, get_diary, save_diary, update_diary_content_only};
    use crate::stream::create_mock_stream;
    use crate::storages::remote_manifest_key;

    /// 创建带测试密钥的 Crypto 实例（使用与 .env 相同的测试凭据）
    fn make_crypto() -> Crypto {
        dotenvy::dotenv().ok();
        let password = std::env::var("TEST_PASSWORD").unwrap_or_else(|_| "1".to_string());
        let salt = std::env::var("TEST_SALT").unwrap_or_else(|_| "NFI2cXl3cUpiSDk4bVVkdEY4cDMzRzlqcTdMMkY5WDg".to_string());
        let crypto = Crypto::new();
        crypto.derive_dek(password, &salt).expect("派生密钥失败");
        crypto
    }

    fn make_lfc() -> (LocalFileCache, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        (LocalFileCache::new(temp_dir.path().to_path_buf()), temp_dir)
    }

    fn make_local_store() -> (LocalStore, LocalFileCache, tempfile::TempDir) {
        let (lfc, td) = make_lfc();
        (LocalStore::new(lfc.clone()), lfc, td)
    }

    // ==================== LocalStore 基础 CRUD ====================

    #[tokio::test]
    async fn test_local_store_manifest_roundtrip() {
        let (store, _lfc, _td) = make_local_store();
        let data = b"encrypted manifest data";

        let etag = store.upload_manifest("test-id-1", data).await.unwrap();
        assert!(!etag.is_empty());

        let (downloaded, returned_etag) = store.download_manifest("test-id-1").await.unwrap();
        assert_eq!(downloaded, data);
        assert_eq!(returned_etag, etag);
    }

    #[tokio::test]
    async fn test_local_store_get_manifest_etag() {
        let (store, _lfc, _td) = make_local_store();

        // 不存在时返回 None
        let etag = store.get_manifest_etag("nonexistent").await.unwrap();
        assert!(etag.is_none());

        // 上传后返回 Some(etag)
        let data = b"test data";
        let uploaded_etag = store.upload_manifest("test-id", data).await.unwrap();
        let etag = store.get_manifest_etag("test-id").await.unwrap();
        assert_eq!(etag, Some(uploaded_etag));
    }

    #[tokio::test]
    async fn test_local_store_delete_diary_all() {
        let (store, _lfc, _td) = make_local_store();

        // 上传 manifest 和附件
        store.upload_manifest("del-test", b"manifest data").await.unwrap();
        store.upload_attachment("del-test", "att1.txt", 5, "text/plain", create_mock_stream(b"hello".to_vec(), 5)).await.unwrap();
        store.upload_attachment("del-test", "att2.txt", 5, "text/plain", create_mock_stream(b"world".to_vec(), 5)).await.unwrap();

        // 验证存在
        assert!(store.get_manifest_etag("del-test").await.unwrap().is_some());

        // 删除
        store.delete_diary_all("del-test").await.unwrap();

        // 验证全部清除
        assert!(store.get_manifest_etag("del-test").await.unwrap().is_none());
        assert!(store.download_attachment("del-test", "att1.txt", None, None).await.is_err());
        assert!(store.download_attachment("del-test", "att2.txt", None, None).await.is_err());
    }

    #[tokio::test]
    async fn test_local_store_list_diary_ids() {
        let (store, _lfc, _td) = make_local_store();

        // 空列表
        let (ids, next) = store.list_diary_ids(None).await.unwrap();
        assert!(ids.is_empty());
        assert!(next.is_none());

        // 创建几个日记
        store.upload_manifest("20250101000000000", b"diary1").await.unwrap();
        store.upload_manifest("20250102000000000", b"diary2").await.unwrap();
        store.upload_manifest("20250103000000000", b"diary3").await.unwrap();

        let (ids, next) = store.list_diary_ids(None).await.unwrap();
        assert_eq!(ids.len(), 3);
        assert!(next.is_none());
        // 应该按降序排列
        assert_eq!(ids[0], "20250103000000000");
        assert_eq!(ids[1], "20250102000000000");
        assert_eq!(ids[2], "20250101000000000");
    }

    #[tokio::test]
    async fn test_local_store_list_diary_ids_pagination() {
        let (store, _lfc, _td) = make_local_store();

        // 创建 5 个日记
        for i in 0..5 {
            let id = format!("id_{:03}", i);
            store.upload_manifest(&id, b"data").await.unwrap();
        }

        // 第一页（page_size = 50，所以全部返回）
        let (ids, next) = store.list_diary_ids(None).await.unwrap();
        assert_eq!(ids.len(), 5);
        assert!(next.is_none());
    }

    // ==================== LocalStore 附件操作 ====================

    #[tokio::test]
    async fn test_local_store_attachment_roundtrip() {
        let (store, _lfc, _td) = make_local_store();
        let data = b"attachment content here";

        let etag = store.upload_attachment("diary1", "photo.jpg", data.len() as u64, "image/jpeg", create_mock_stream(data.to_vec(), data.len())).await.unwrap();
        assert!(!etag.is_empty());

        let (stream, _len) = store.download_attachment("diary1", "photo.jpg", None, None).await.unwrap();
        let downloaded = crate::stream::collect_data(stream).await.unwrap();
        assert_eq!(downloaded, data);
    }

    #[tokio::test]
    async fn test_local_store_delete_attachment() {
        let (store, _lfc, _td) = make_local_store();

        store.upload_attachment("diary1", "file.txt", 4, "text/plain", create_mock_stream(b"test".to_vec(), 4)).await.unwrap();

        // 确认存在
        assert!(store.download_attachment("diary1", "file.txt", None, None).await.is_ok());

        // 删除
        store.delete_attachment("diary1", "file.txt").await.unwrap();

        // 确认不存在
        assert!(store.download_attachment("diary1", "file.txt", None, None).await.is_err());
    }

    #[tokio::test]
    async fn test_local_store_rename_attachment() {
        let (store, _lfc, _td) = make_local_store();
        let data = b"rename test data";

        store.upload_attachment("diary1", "old.txt", data.len() as u64, "text/plain", create_mock_stream(data.to_vec(), data.len())).await.unwrap();

        store.rename_attachment("diary1", "old.txt", "new.txt").await.unwrap();

        // 旧名字不存在
        assert!(store.download_attachment("diary1", "old.txt", None, None).await.is_err());
        // 新名字存在且数据正确
        let (stream, _) = store.download_attachment("diary1", "new.txt", None, None).await.unwrap();
        let downloaded = crate::stream::collect_data(stream).await.unwrap();
        assert_eq!(downloaded, data);
    }

    #[tokio::test]
    async fn test_local_store_get_attachment_url() {
        let (store, _lfc, _td) = make_local_store();
        let meta = AttachmentMeta {
            filename: "test.jpg".to_string(),
            mimetype: "image/jpeg".to_string(),
            size: 100,
            encrypted: true,
            nonce: vec![],
            algorithm: crate::cryptos::crypto_types::EncryptionAlgorithm::Ctr,
            etag: None,
        };
        let url = store.get_attachment_url("diary1", &meta).await.unwrap();
        assert!(url.contains("attachment.localhost"));
        assert!(url.contains("diary1"));
        assert!(url.contains("test.jpg"));
    }

    // ==================== LocalStore + 日记函数集成 ====================

    #[tokio::test]
    async fn test_local_store_save_and_get_diary() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _lfc, _td) = make_local_store();

        let (summary, content) = save_diary(&cache, &crypto, &store, "Hello, local world!").await.unwrap();
        assert_eq!(content, "Hello, local world!");
        assert!(!summary.id.is_empty());

        let manifest = get_diary(&cache, &crypto, &store, &summary.id).await.unwrap();
        assert_eq!(manifest.content, "Hello, local world!");
        assert_eq!(manifest.attachments.len(), 0);
    }

    #[tokio::test]
    async fn test_local_store_update_diary() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _lfc, _td) = make_local_store();

        let (summary, _) = save_diary(&cache, &crypto, &store, "original").await.unwrap();

        let updated = update_diary_content_only(&cache, &crypto, &store, &summary.id, "updated content").await.unwrap();
        assert!(updated.updated >= summary.updated);

        let manifest = get_diary(&cache, &crypto, &store, &summary.id).await.unwrap();
        assert_eq!(manifest.content, "updated content");
    }

    #[tokio::test]
    async fn test_local_store_delete_diary() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _lfc, _td) = make_local_store();

        let (summary, _) = save_diary(&cache, &crypto, &store, "to be deleted").await.unwrap();
        let id = summary.id.clone();

        // 确认存在
        assert!(get_diary(&cache, &crypto, &store, &id).await.is_ok());

        // 删除
        delete_diary(&cache, &store, &id).await.unwrap();

        // 确认不存在
        assert!(get_diary(&cache, &crypto, &store, &id).await.is_err());
    }

    #[tokio::test]
    async fn test_local_store_list_after_save() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _lfc, _td) = make_local_store();

        save_diary(&cache, &crypto, &store, "diary one").await.unwrap();
        save_diary(&cache, &crypto, &store, "diary two").await.unwrap();
        save_diary(&cache, &crypto, &store, "diary three").await.unwrap();

        let (ids, _) = store.list_diary_ids(None).await.unwrap();
        assert_eq!(ids.len(), 3);
    }

    #[tokio::test]
    async fn test_local_store_get_nonexistent_diary() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _lfc, _td) = make_local_store();

        let result = get_diary(&cache, &crypto, &store, "nonexistent-id").await;
        assert!(result.is_err());
    }

    // ==================== AppState.diary_store() 行为 ====================

    #[test]
    fn test_app_state_diary_store_local_mode() {
        let (lfc, _td) = make_lfc();
        let state = crate::state::AppState::from_parts(
            Crypto::new(),
            OssClient::new(),
            lfc,
        );
        // 默认 remote_enabled = false
        assert!(!state.is_remote_enabled());
    }

    #[test]
    fn test_app_state_diary_store_remote_mode() {
        let (lfc, _td) = make_lfc();
        let state = crate::state::AppState::from_parts(
            Crypto::new(),
            OssClient::new(),
            lfc,
        );
        state.set_remote_enabled(true);
        assert!(state.is_remote_enabled());

        state.set_remote_enabled(false);
        assert!(!state.is_remote_enabled());
    }

    #[tokio::test]
    async fn test_local_store_data_persistence_across_instances() {
        let (lfc, _td) = make_lfc();

        // 用第一个 store 实例写入
        {
            let store = LocalStore::new(lfc.clone());
            store.upload_manifest("persist-test", b"persistent data").await.unwrap();
        }

        // 用第二个 store 实例读取（模拟应用重启）
        {
            let store = LocalStore::new(lfc.clone());
            let (data, _etag) = store.download_manifest("persist-test").await.unwrap();
            assert_eq!(data, b"persistent data");
        }
    }

    // ==================== 边界情况 ====================

    #[tokio::test]
    async fn test_local_store_empty_manifest() {
        let (store, _lfc, _td) = make_local_store();
        let etag = store.upload_manifest("empty", b"").await.unwrap();
        let (data, _) = store.download_manifest("empty").await.unwrap();
        assert!(data.is_empty());
        assert!(!etag.is_empty());
    }

    #[tokio::test]
    async fn test_local_store_overwrite_manifest() {
        let (store, _lfc, _td) = make_local_store();

        store.upload_manifest("overwrite", b"version 1").await.unwrap();
        store.upload_manifest("overwrite", b"version 2").await.unwrap();

        let (data, _) = store.download_manifest("overwrite").await.unwrap();
        assert_eq!(data, b"version 2");
    }

    #[tokio::test]
    async fn test_local_store_attachment_with_range() {
        let (store, _lfc, _td) = make_local_store();
        let data = b"0123456789abcdef";

        store.upload_attachment("diary1", "range-test", data.len() as u64, "application/octet-stream", create_mock_stream(data.to_vec(), data.len())).await.unwrap();

        // 请求 range [4, 8]
        let (stream, _) = store.download_attachment("diary1", "range-test", Some((4, 8)), None).await.unwrap();
        let downloaded = crate::stream::collect_data(stream).await.unwrap();
        assert_eq!(downloaded, b"45678");
    }

    // ==================== 跨 store 数据迁移模拟 ====================

    #[tokio::test]
    async fn test_local_to_remote_data_transfer() {
        let (lfc, _td) = make_lfc();
        let local_store = LocalStore::new(lfc.clone());

        // 在 LocalStore 中创建数据
        local_store.upload_manifest("migrate-1", b"manifest data").await.unwrap();
        local_store.upload_attachment("migrate-1", "att.txt", 4, "text/plain", create_mock_stream(b"test".to_vec(), 4)).await.unwrap();

        // 模拟从 LocalStore 读取（同步到云端的第一步）
        let (manifest, _etag) = local_store.download_manifest("migrate-1").await.unwrap();
        assert_eq!(manifest, b"manifest data");

        let (att_stream, _) = local_store.download_attachment("migrate-1", "att.txt", None, None).await.unwrap();
        let att_data = crate::stream::collect_data(att_stream).await.unwrap();
        assert_eq!(att_data, b"test");
    }
}
