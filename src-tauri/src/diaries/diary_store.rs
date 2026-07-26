use async_trait::async_trait;

use crate::caches::{CacheError, LocalFileCache};
use crate::diaries::attachment_upload::{LocalAttachmentUpload, RemoteAttachmentUpload};
use crate::diaries::{AttachmentUploadSession, DiaryError};
use crate::object::{NextToken, ObjectError, ObjectMigrationOutcome, OssClient};
use crate::storages::{diary_id_from_manifest_key, remote_attachments_key, remote_manifest_key};
use crate::stream::{tracker_stream, ByteStream};
use std::sync::Arc;

pub type StoreProgressCallback = Arc<dyn Fn(u8) + Send + Sync>;

fn diary_object_keys(keys: impl IntoIterator<Item = String>, id: &str) -> (Vec<String>, String) {
    let prefix = format!("{id}/");
    let manifest_key = remote_manifest_key(id);
    let mut attachment_keys: Vec<String> = keys
        .into_iter()
        .filter(|key| key.starts_with(&prefix) && key != &manifest_key)
        .collect();
    attachment_keys.sort();
    (attachment_keys, manifest_key)
}

async fn delete_local_diary_files(lfc: &LocalFileCache, id: &str) -> Result<(), DiaryError> {
    let entries = match lfc.get_all().await {
        Ok(entries) => entries,
        // 尚未产生任何本地缓存时，LFC 目录可能还没有创建；删除应保持幂等。
        Err(CacheError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    let keys = entries.into_iter().map(|(key, _)| key);
    let (attachment_keys, manifest_key) = diary_object_keys(keys, id);

    // manifest 是日记是否存在的提交标志。只有附件全部删除成功后才删除它，
    // 这样失败时日记仍可见且可以安全重试。
    for key in attachment_keys {
        lfc.delete(&key).await?;
    }
    lfc.delete(&manifest_key).await?;
    Ok(())
}

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
    /// 创建附件分片写入会话；会话负责当前存储模式下的落盘、远端上传与回滚。
    async fn begin_attachment_upload(
        &self,
        id: &str,
        filename: &str,
        size: u64,
        mimetype: &str,
    ) -> Result<Box<dyn AttachmentUploadSession>, DiaryError>;
    /// 获取附件流，支持 Range 请求
    async fn download_attachment(
        &self,
        id: &str,
        filename: &str,
        range: Option<(u64, u64)>,
        known_etag: Option<&str>,
    ) -> Result<ByteStream, DiaryError>;
    /// 将完整附件缓存到本地；已经命中有效缓存时直接成功。
    async fn cache_attachment(
        &self,
        id: &str,
        filename: &str,
        progress: StoreProgressCallback,
    ) -> Result<(), DiaryError>;
    /// 删除附件
    async fn delete_attachment(&self, id: &str, filename: &str) -> Result<(), DiaryError>;
    /// 将 V3 的 filename 对象 key 幂等迁移为 V4 attachment ID。
    async fn migrate_attachment_object(
        &self,
        id: &str,
        old_filename: &str,
        attachment_id: &str,
    ) -> Result<ObjectMigrationOutcome, DiaryError>;
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
        delete_local_diary_files(&self.lfc, id).await
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
        // 日记 ID 是反向时间戳：时间越新，ID 的字典序越小。
        // 因此升序排列才是“最新日记在前”，并与 OSS 的对象键顺序一致。
        ids.sort();

        // 简单分页：next_token 编码为偏移量
        let offset: usize = next_token.and_then(|t| t.parse().ok()).unwrap_or(0);
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
        let data = crate::stream::collect_data(stream).await.map_err(|e| {
            DiaryError::Object(crate::object::ObjectError::OperationFailed(e.to_string()))
        })?;
        self.lfc.save_bytes(&key, &data).await?;
        let etag = format!("{:X}", md5::compute(&data));
        Ok(etag)
    }

    async fn begin_attachment_upload(
        &self,
        id: &str,
        filename: &str,
        size: u64,
        _mimetype: &str,
    ) -> Result<Box<dyn AttachmentUploadSession>, DiaryError> {
        let key = remote_attachments_key(id, filename);
        Ok(Box::new(
            LocalAttachmentUpload::begin(self.lfc.clone(), key, size).await?,
        ))
    }

    async fn download_attachment(
        &self,
        id: &str,
        filename: &str,
        range: Option<(u64, u64)>,
        _known_etag: Option<&str>,
    ) -> Result<ByteStream, DiaryError> {
        let key = remote_attachments_key(id, filename);
        Ok(self.lfc.get_stream(&key, range).await?)
    }

    async fn cache_attachment(
        &self,
        id: &str,
        filename: &str,
        progress: StoreProgressCallback,
    ) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        self.lfc
            .get(&key)
            .await?
            .ok_or(crate::caches::CacheError::NotFound)?;
        progress(100);
        Ok(())
    }

    async fn delete_attachment(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        self.lfc.delete(&key).await?;
        Ok(())
    }

    async fn migrate_attachment_object(
        &self,
        id: &str,
        old_filename: &str,
        attachment_id: &str,
    ) -> Result<ObjectMigrationOutcome, DiaryError> {
        let old_key = remote_attachments_key(id, old_filename);
        let new_key = remote_attachments_key(id, attachment_id);
        if old_key == new_key {
            return Ok(ObjectMigrationOutcome::AlreadyMigrated);
        }
        let old_etag = self.lfc.get(&old_key).await?;
        let new_etag = self.lfc.get(&new_key).await?;
        match (old_etag, new_etag) {
            (None, None) => Ok(ObjectMigrationOutcome::Missing),
            (None, Some(_)) => Ok(ObjectMigrationOutcome::AlreadyMigrated),
            (Some(old_etag), Some(new_etag)) => {
                if !etags_match(&old_etag, &new_etag) {
                    return Err(DiaryError::Object(ObjectError::KeyAlreadyExists(new_key)));
                }
                self.lfc.delete(&old_key).await?;
                Ok(ObjectMigrationOutcome::AlreadyMigrated)
            }
            (Some(old_etag), None) => {
                let stream = self.lfc.get_stream(&old_key, None).await?;
                self.lfc
                    .save_stream_with_etag(&new_key, &old_etag, stream)
                    .await?;
                self.lfc.delete(&old_key).await?;
                Ok(ObjectMigrationOutcome::Migrated)
            }
        }
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
        let prefix = format!("{id}/");
        let keys = self.client.list_all_keys(&prefix).await?;
        let (attachment_keys, manifest_key) = diary_object_keys(keys, id);

        self.client.delete_keys(attachment_keys).await?;
        self.client.delete(&manifest_key).await?;

        // 远端是权威来源；远端完整删除后再清理本地副本。缓存清理失败也要
        // 返回错误，使用户可以重试，避免切换本地模式后旧日记重新出现。
        delete_local_diary_files(&self.lfc, id).await
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
        match self
            .client
            .upload(&key, size, wrapped_stream, mimetype)
            .await
        {
            Ok(etag) => {
                if let Err(cache_error) = handle.finalize(&etag).await {
                    let _ = self.lfc.delete(&key).await;
                    if let Err(rollback_error) = self.client.delete(&key).await {
                        return Err(DiaryError::Object(ObjectError::OperationFailed(format!(
                            "本地附件固化失败：{cache_error}；远端回滚失败：{rollback_error}"
                        ))));
                    }
                    return Err(cache_error.into());
                }
                Ok(etag)
            }
            Err(e) => {
                handle.abort().await;
                Err(DiaryError::Object(e))
            }
        }
    }

    async fn begin_attachment_upload(
        &self,
        id: &str,
        filename: &str,
        size: u64,
        mimetype: &str,
    ) -> Result<Box<dyn AttachmentUploadSession>, DiaryError> {
        let key = remote_attachments_key(id, filename);
        Ok(Box::new(
            RemoteAttachmentUpload::begin(
                self.lfc.clone(),
                self.client.clone(),
                key,
                size,
                mimetype.to_string(),
            )
            .await?,
        ))
    }

    async fn download_attachment(
        &self,
        id: &str,
        filename: &str,
        range: Option<(u64, u64)>,
        known_etag: Option<&str>,
    ) -> Result<ByteStream, DiaryError> {
        let key = remote_attachments_key(id, filename);
        if let Some(cached_etag) = self.lfc.get(&key).await? {
            // 已知 etag 匹配，直接使用缓存
            if known_etag.is_some_and(|k| k == cached_etag) {
                return Ok(self.lfc.get_stream(&key, range).await?);
            }
            let metadata = self.client.get_metadata(&key).await?;
            if metadata.etag.as_deref() == Some(&cached_etag) {
                return Ok(self.lfc.get_stream(&key, range).await?);
            } else {
                let _ = self.lfc.delete(&key).await;
            }
        }
        let (stream, _) = self.client.download(&key, range).await?;
        Ok(stream)
    }

    async fn cache_attachment(
        &self,
        id: &str,
        filename: &str,
        progress: StoreProgressCallback,
    ) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        let metadata = self.client.get_metadata(&key).await?;
        let remote_etag = metadata.etag.ok_or_else(|| {
            DiaryError::Object(ObjectError::OperationFailed(format!(
                "附件缺少 ETag: {key}"
            )))
        })?;

        if self
            .lfc
            .get(&key)
            .await?
            .is_some_and(|cached_etag| etags_match(&cached_etag, &remote_etag))
        {
            progress(100);
            return Ok(());
        }

        let (stream, size) = self.client.download(&key, None).await?;
        let progress_for_stream = progress.clone();
        let tracked_stream = tracker_stream(size, stream, move |percent| {
            progress_for_stream(percent);
        });
        self.lfc
            .save_stream_with_etag(&key, &remote_etag, tracked_stream)
            .await?;
        progress(100);
        Ok(())
    }

    async fn delete_attachment(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        self.client.delete(&key).await?;
        let _ = self.lfc.delete(&key).await;
        Ok(())
    }

    async fn migrate_attachment_object(
        &self,
        id: &str,
        old_filename: &str,
        attachment_id: &str,
    ) -> Result<ObjectMigrationOutcome, DiaryError> {
        let old_key = remote_attachments_key(id, old_filename);
        let new_key = remote_attachments_key(id, attachment_id);
        let outcome = self.client.migrate_object(&old_key, &new_key).await?;
        // OSS 是远程模式下的权威来源；清理两个位置的缓存，避免保留半迁移状态。
        let _ = self.lfc.delete(&old_key).await;
        let _ = self.lfc.delete(&new_key).await;
        Ok(outcome)
    }
}

fn etags_match(left: &str, right: &str) -> bool {
    left.trim_matches('"')
        .eq_ignore_ascii_case(right.trim_matches('"'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::DiaryMemoryCache;
    use crate::cryptos::crypto_types::EncryptionAlgorithm::{Ctr, Gcm};
    use crate::cryptos::Crypto;
    use crate::diaries::diary::{delete_diary, get_diary, save_diary, update_diary_content_only};
    use crate::diaries::diary_content::DiaryContentNode;
    use crate::diaries::diary_migration::{legacy_attachment_id, CURRENT_VERSION};
    use crate::stream::create_mock_stream;
    use crate::utils::id_generate::generate_descending_id_with_timestamp;
    use std::sync::Mutex;

    /// 创建带测试密钥的 Crypto 实例（使用与 .env 相同的测试凭据）
    fn make_crypto() -> Crypto {
        dotenvy::dotenv().ok();
        let password = std::env::var("TEST_PASSWORD").unwrap_or_else(|_| "1".to_string());
        let salt = std::env::var("TEST_SALT")
            .unwrap_or_else(|_| "NFI2cXl3cUpiSDk4bVVkdEY4cDMzRzlqcTdMMkY5WDg".to_string());
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
        store
            .upload_manifest("del-test", b"manifest data")
            .await
            .unwrap();
        store
            .upload_attachment(
                "del-test",
                "att1.txt",
                5,
                "text/plain",
                create_mock_stream(b"hello".to_vec(), 5),
            )
            .await
            .unwrap();
        store
            .upload_attachment(
                "del-test",
                "att2.txt",
                5,
                "text/plain",
                create_mock_stream(b"world".to_vec(), 5),
            )
            .await
            .unwrap();

        // 验证存在
        assert!(store.get_manifest_etag("del-test").await.unwrap().is_some());

        // 删除
        store.delete_diary_all("del-test").await.unwrap();

        // 验证全部清除
        assert!(store.get_manifest_etag("del-test").await.unwrap().is_none());
        assert!(store
            .download_attachment("del-test", "att1.txt", None, None)
            .await
            .is_err());
        assert!(store
            .download_attachment("del-test", "att2.txt", None, None)
            .await
            .is_err());
    }

    #[test]
    fn diary_delete_plan_scopes_prefix_and_keeps_manifest_separate() {
        let id = "1234567890123";
        let manifest = remote_manifest_key(id);
        let (attachments, manifest_key) = diary_object_keys(
            vec![
                manifest.clone(),
                format!("{id}/att-b"),
                format!("{id}/att-a"),
                format!("{id}4/att-unrelated"),
                "unrelated/manifest.enc".to_string(),
            ],
            id,
        );

        assert_eq!(
            attachments,
            vec![format!("{id}/att-a"), format!("{id}/att-b")]
        );
        assert_eq!(manifest_key, manifest);
    }

    #[tokio::test]
    async fn local_delete_is_idempotent_without_cache_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().join("missing"));
        let store = LocalStore::new(lfc);

        store.delete_diary_all("missing-diary").await.unwrap();
    }

    #[tokio::test]
    async fn local_delete_propagates_cache_scan_errors() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_file = temp_dir.path().join("not-a-directory");
        tokio::fs::write(&cache_file, b"file").await.unwrap();
        let store = LocalStore::new(LocalFileCache::new(cache_file));

        assert!(matches!(
            store.delete_diary_all("diary").await,
            Err(DiaryError::Cache(CacheError::Io(_)))
        ));
    }

    #[tokio::test]
    async fn test_local_store_list_diary_ids() {
        let (store, _lfc, _td) = make_local_store();

        // 空列表
        let (ids, next) = store.list_diary_ids(None).await.unwrap();
        assert!(ids.is_empty());
        assert!(next.is_none());

        // 使用真实的反向时间戳 ID，并故意打乱写入顺序。
        let oldest = generate_descending_id_with_timestamp(1_700_000_000_000);
        let middle = generate_descending_id_with_timestamp(1_700_000_001_000);
        let newest = generate_descending_id_with_timestamp(1_700_000_002_000);
        store.upload_manifest(&middle, b"diary2").await.unwrap();
        store.upload_manifest(&oldest, b"diary1").await.unwrap();
        store.upload_manifest(&newest, b"diary3").await.unwrap();

        let (ids, next) = store.list_diary_ids(None).await.unwrap();
        assert_eq!(ids, vec![newest, middle, oldest]);
        assert!(next.is_none());
    }

    #[tokio::test]
    async fn test_local_store_list_diary_ids_pagination() {
        let (store, _lfc, _td) = make_local_store();

        // 跨过 50 条的分页边界，验证每一页拼接后仍保持最新在前。
        let mut expected = Vec::new();
        for i in 0..55 {
            let id = generate_descending_id_with_timestamp(1_700_000_000_000 + i);
            store.upload_manifest(&id, b"data").await.unwrap();
            expected.push(id);
        }
        expected.sort();

        let (first_page, next) = store.list_diary_ids(None).await.unwrap();
        assert_eq!(first_page, expected[..50]);
        assert_eq!(next.as_deref(), Some("50"));

        let (second_page, next) = store.list_diary_ids(next).await.unwrap();
        assert_eq!(second_page, expected[50..]);
        assert!(next.is_none());
    }

    // ==================== LocalStore 附件操作 ====================

    #[tokio::test]
    async fn test_local_store_attachment_roundtrip() {
        let (store, _lfc, _td) = make_local_store();
        let data = b"attachment content here";

        let etag = store
            .upload_attachment(
                "diary1",
                "photo.jpg",
                data.len() as u64,
                "image/jpeg",
                create_mock_stream(data.to_vec(), data.len()),
            )
            .await
            .unwrap();
        assert!(!etag.is_empty());

        let stream = store
            .download_attachment("diary1", "photo.jpg", None, None)
            .await
            .unwrap();
        let downloaded = crate::stream::collect_data(stream).await.unwrap();
        assert_eq!(downloaded, data);
    }

    #[tokio::test]
    async fn test_local_store_cache_attachment_requires_existing_cache() {
        let (store, _lfc, _td) = make_local_store();
        let progress_values = Arc::new(Mutex::new(Vec::new()));

        let missing_result = store
            .cache_attachment("diary1", "missing.txt", Arc::new(|_| {}))
            .await;
        assert!(missing_result.is_err());

        store
            .upload_attachment(
                "diary1",
                "cached.txt",
                4,
                "text/plain",
                create_mock_stream(b"test".to_vec(), 4),
            )
            .await
            .unwrap();

        let captured_progress = progress_values.clone();
        store
            .cache_attachment(
                "diary1",
                "cached.txt",
                Arc::new(move |progress| {
                    captured_progress.lock().unwrap().push(progress);
                }),
            )
            .await
            .unwrap();

        assert_eq!(*progress_values.lock().unwrap(), vec![100]);
    }

    #[tokio::test]
    async fn test_local_store_delete_attachment() {
        let (store, _lfc, _td) = make_local_store();

        store
            .upload_attachment(
                "diary1",
                "file.txt",
                4,
                "text/plain",
                create_mock_stream(b"test".to_vec(), 4),
            )
            .await
            .unwrap();

        // 确认存在
        assert!(store
            .download_attachment("diary1", "file.txt", None, None)
            .await
            .is_ok());

        // 删除
        store.delete_attachment("diary1", "file.txt").await.unwrap();

        // 确认不存在
        assert!(store
            .download_attachment("diary1", "file.txt", None, None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_local_store_migrates_attachment_object_idempotently() {
        let (store, _lfc, _td) = make_local_store();
        let data = b"rename test data";

        store
            .upload_attachment(
                "diary1",
                "old.txt",
                data.len() as u64,
                "text/plain",
                create_mock_stream(data.to_vec(), data.len()),
            )
            .await
            .unwrap();

        assert_eq!(
            store
                .migrate_attachment_object("diary1", "old.txt", "att-stable")
                .await
                .unwrap(),
            ObjectMigrationOutcome::Migrated
        );

        // 旧名字不存在
        assert!(store
            .download_attachment("diary1", "old.txt", None, None)
            .await
            .is_err());
        // 新名字存在且数据正确
        let stream = store
            .download_attachment("diary1", "att-stable", None, None)
            .await
            .unwrap();
        let downloaded = crate::stream::collect_data(stream).await.unwrap();
        assert_eq!(downloaded, data);

        assert_eq!(
            store
                .migrate_attachment_object("diary1", "old.txt", "att-stable")
                .await
                .unwrap(),
            ObjectMigrationOutcome::AlreadyMigrated
        );
    }

    #[tokio::test]
    async fn test_local_store_rejects_conflicting_migration_target() {
        let (store, _lfc, _td) = make_local_store();
        for (key, data) in [
            ("old.txt", b"old".as_slice()),
            ("att-stable", b"new".as_slice()),
        ] {
            store
                .upload_attachment(
                    "diary1",
                    key,
                    data.len() as u64,
                    "text/plain",
                    create_mock_stream(data.to_vec(), data.len()),
                )
                .await
                .unwrap();
        }

        assert!(matches!(
            store
                .migrate_attachment_object("diary1", "old.txt", "att-stable")
                .await,
            Err(DiaryError::Object(ObjectError::KeyAlreadyExists(_)))
        ));
        assert!(store
            .download_attachment("diary1", "old.txt", None, None)
            .await
            .is_ok());
        assert!(store
            .download_attachment("diary1", "att-stable", None, None)
            .await
            .is_ok());
    }

    // ==================== LocalStore + 日记函数集成 ====================

    #[tokio::test]
    async fn test_local_store_save_and_get_diary() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _lfc, _td) = make_local_store();

        let (summary, content) = save_diary(&cache, &crypto, &store, "Hello, local world!")
            .await
            .unwrap();
        assert_eq!(content.searchable_text(), "Hello, local world!");
        assert!(!summary.id.is_empty());

        let manifest = get_diary(&cache, &crypto, &store, &summary.id)
            .await
            .unwrap();
        assert_eq!(manifest.content.searchable_text(), "Hello, local world!");
        assert_eq!(manifest.attachments.len(), 0);
    }

    #[tokio::test]
    async fn test_get_diary_commits_v4_only_after_object_migration_can_succeed() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _lfc, _td) = make_local_store();
        let diary_id = "v3-object-migration";
        let old_filename = "photo.jpg";
        let attachment_id = legacy_attachment_id(diary_id, old_filename);
        let original = b"original-photo";
        store
            .upload_attachment(
                diary_id,
                old_filename,
                original.len() as u64,
                "image/jpeg",
                create_mock_stream(original.to_vec(), original.len()),
            )
            .await
            .unwrap();
        // 先制造目标 key 冲突，验证迁移失败时不会发布 V4 manifest。
        store
            .upload_attachment(
                diary_id,
                &attachment_id,
                8,
                "image/jpeg",
                create_mock_stream(b"conflict".to_vec(), 8),
            )
            .await
            .unwrap();
        let v3 = serde_json::json!({
            "id": diary_id,
            "algorithm": Gcm,
            "content": {
                "nodes": [{
                    "type": "image",
                    "filename": old_filename,
                    "size": "normal"
                }]
            },
            "created": 1,
            "updated": 1,
            "attachments": [{
                "filename": old_filename,
                "mimetype": "image/jpeg",
                "size": original.len(),
                "encrypted": false,
                "nonce": [],
                "algorithm": Ctr,
                "etag": null
            }],
            "version": 3
        });
        let encrypted_v3 = crypto.encrypt(&serde_json::to_vec(&v3).unwrap()).unwrap();
        store
            .upload_manifest(diary_id, &encrypted_v3)
            .await
            .unwrap();

        assert!(get_diary(&cache, &crypto, &store, diary_id).await.is_err());
        let (still_v3, _) = store.download_manifest(diary_id).await.unwrap();
        let still_v3: serde_json::Value =
            serde_json::from_slice(&crypto.decrypt(&still_v3).unwrap()).unwrap();
        assert_eq!(still_v3["version"], 3);

        store
            .delete_attachment(diary_id, &attachment_id)
            .await
            .unwrap();
        let migrated = get_diary(&cache, &crypto, &store, diary_id).await.unwrap();
        assert_eq!(migrated.version, CURRENT_VERSION);
        assert_eq!(migrated.attachments[0].id, attachment_id);
        assert_eq!(migrated.attachments[0].filename, old_filename);
        assert!(matches!(
            &migrated.content.nodes[0],
            DiaryContentNode::Image {
                attachment_id: node_id,
                ..
            } if node_id == &attachment_id
        ));
        assert!(store
            .download_attachment(diary_id, old_filename, None, None)
            .await
            .is_err());
        let stream = store
            .download_attachment(diary_id, &attachment_id, None, None)
            .await
            .unwrap();
        assert_eq!(crate::stream::collect_data(stream).await.unwrap(), original);
        let (v4_bytes, _) = store.download_manifest(diary_id).await.unwrap();
        let v4: serde_json::Value =
            serde_json::from_slice(&crypto.decrypt(&v4_bytes).unwrap()).unwrap();
        assert_eq!(v4["version"], CURRENT_VERSION);
    }

    #[tokio::test]
    async fn test_local_store_update_diary() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _lfc, _td) = make_local_store();

        let (summary, _) = save_diary(&cache, &crypto, &store, "original")
            .await
            .unwrap();

        let updated =
            update_diary_content_only(&cache, &crypto, &store, &summary.id, "updated content")
                .await
                .unwrap();
        assert!(updated.updated >= summary.updated);

        let manifest = get_diary(&cache, &crypto, &store, &summary.id)
            .await
            .unwrap();
        assert_eq!(manifest.content.searchable_text(), "updated content");
    }

    #[tokio::test]
    async fn test_local_store_delete_diary() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _lfc, _td) = make_local_store();

        let (summary, _) = save_diary(&cache, &crypto, &store, "to be deleted")
            .await
            .unwrap();
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

        save_diary(&cache, &crypto, &store, "diary one")
            .await
            .unwrap();
        save_diary(&cache, &crypto, &store, "diary two")
            .await
            .unwrap();
        save_diary(&cache, &crypto, &store, "diary three")
            .await
            .unwrap();

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
        let state = crate::state::AppState::from_parts(Crypto::new(), OssClient::new(), lfc);
        // 默认 remote_enabled = false
        assert!(!state.is_remote_enabled());
    }

    #[test]
    fn test_app_state_diary_store_remote_mode() {
        let (lfc, _td) = make_lfc();
        let state = crate::state::AppState::from_parts(Crypto::new(), OssClient::new(), lfc);
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
            store
                .upload_manifest("persist-test", b"persistent data")
                .await
                .unwrap();
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

        store
            .upload_manifest("overwrite", b"version 1")
            .await
            .unwrap();
        store
            .upload_manifest("overwrite", b"version 2")
            .await
            .unwrap();

        let (data, _) = store.download_manifest("overwrite").await.unwrap();
        assert_eq!(data, b"version 2");
    }

    #[tokio::test]
    async fn test_local_store_attachment_with_range() {
        let (store, _lfc, _td) = make_local_store();
        let data = b"0123456789abcdef";

        store
            .upload_attachment(
                "diary1",
                "range-test",
                data.len() as u64,
                "application/octet-stream",
                create_mock_stream(data.to_vec(), data.len()),
            )
            .await
            .unwrap();

        // 请求 range [4, 8]
        let stream = store
            .download_attachment("diary1", "range-test", Some((4, 8)), None)
            .await
            .unwrap();
        let downloaded = crate::stream::collect_data(stream).await.unwrap();
        assert_eq!(downloaded, b"45678");
    }

    // ==================== 跨 store 数据迁移模拟 ====================

    #[tokio::test]
    async fn test_local_to_remote_data_transfer() {
        let (lfc, _td) = make_lfc();
        let local_store = LocalStore::new(lfc.clone());

        // 在 LocalStore 中创建数据
        local_store
            .upload_manifest("migrate-1", b"manifest data")
            .await
            .unwrap();
        local_store
            .upload_attachment(
                "migrate-1",
                "att.txt",
                4,
                "text/plain",
                create_mock_stream(b"test".to_vec(), 4),
            )
            .await
            .unwrap();

        // 模拟从 LocalStore 读取（同步到云端的第一步）
        let (manifest, _etag) = local_store.download_manifest("migrate-1").await.unwrap();
        assert_eq!(manifest, b"manifest data");

        let att_stream = local_store
            .download_attachment("migrate-1", "att.txt", None, None)
            .await
            .unwrap();
        let att_data = crate::stream::collect_data(att_stream).await.unwrap();
        assert_eq!(att_data, b"test");
    }
}
