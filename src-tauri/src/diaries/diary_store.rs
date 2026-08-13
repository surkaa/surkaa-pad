use async_trait::async_trait;

#[cfg(test)]
use crate::app_config::{AppConfig, AppConfigStore};
use crate::caches::{AttachmentCacheManager, CacheError, LocalObjectStore};
use crate::diaries::attachment_upload::{LocalAttachmentUpload, RemoteAttachmentUpload};
use crate::diaries::{AttachmentUploadSession, DiaryError};
use crate::object::{NextToken, ObjectError, OssClient};
use crate::storages::{diary_id_from_manifest_key, remote_attachments_key, remote_manifest_key};
use crate::stream::{tracker_stream, ByteStream};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio_util::io::StreamReader;
use tokio_util::sync::CancellationToken;

pub type StoreProgressCallback = Arc<dyn Fn(u8) + Send + Sync>;
pub type AttachmentUploadProgressCallback = Arc<dyn Fn(AttachmentUploadProgress) + Send + Sync>;

const ATTACHMENT_UPLOAD_CHUNK_SIZE: usize = 8 * 1024 * 1024;

fn attachment_backup_key(id: &str, filename: &str) -> String {
    format!("{id}/.attachment-transaction/{filename}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentUploadProgress {
    Transferring(u8),
    Finalizing,
}

pub struct AttachmentUploadOptions {
    progress: AttachmentUploadProgressCallback,
    cancellation: Option<CancellationToken>,
}

impl AttachmentUploadOptions {
    pub fn new(
        progress: AttachmentUploadProgressCallback,
        cancellation: Option<CancellationToken>,
    ) -> Self {
        Self {
            progress,
            cancellation,
        }
    }
}

struct AttachmentUploadGuard {
    session: Option<Box<dyn AttachmentUploadSession>>,
}

impl AttachmentUploadGuard {
    fn new(session: Box<dyn AttachmentUploadSession>) -> Self {
        Self {
            session: Some(session),
        }
    }

    fn session_mut(&mut self) -> &mut Box<dyn AttachmentUploadSession> {
        self.session.as_mut().expect("附件上传会话已经结束")
    }

    fn take(&mut self) -> Box<dyn AttachmentUploadSession> {
        self.session.take().expect("附件上传会话已经结束")
    }
}

impl Drop for AttachmentUploadGuard {
    fn drop(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            if let Err(error) = session.abort().await {
                tauri_plugin_log::log::error!("附件上传 Future 被中止后的会话清理失败: {error}");
            }
        });
    }
}

fn upload_canceled(cancellation: Option<&CancellationToken>) -> bool {
    cancellation.is_some_and(CancellationToken::is_cancelled)
}

async fn abort_canceled_upload(mut session: AttachmentUploadGuard) -> DiaryError {
    let cleanup = session.take().abort().await;
    match cleanup {
        Ok(()) => DiaryError::AttachmentUpload("附件上传已取消".into()),
        Err(error) => {
            DiaryError::AttachmentUpload(format!("附件上传已取消；清理上传会话失败：{error}"))
        }
    }
}

async fn upload_stream_to_session(
    session: Box<dyn AttachmentUploadSession>,
    expected_size: u64,
    stream: ByteStream,
    progress: AttachmentUploadProgressCallback,
    cancellation: Option<&CancellationToken>,
) -> Result<String, DiaryError> {
    let mut session = AttachmentUploadGuard::new(session);
    let mut reader = StreamReader::new(stream);
    let mut transferred = 0u64;

    loop {
        if upload_canceled(cancellation) {
            return Err(abort_canceled_upload(session).await);
        }
        let mut chunk = Vec::with_capacity(ATTACHMENT_UPLOAD_CHUNK_SIZE);
        while chunk.len() < ATTACHMENT_UPLOAD_CHUNK_SIZE {
            let read_result = if let Some(cancellation) = cancellation {
                tokio::select! {
                    result = reader.read_buf(&mut chunk) => Some(result),
                    _ = cancellation.cancelled() => None,
                }
            } else {
                Some(reader.read_buf(&mut chunk).await)
            };
            let Some(read_result) = read_result else {
                return Err(abort_canceled_upload(session).await);
            };
            match read_result {
                Ok(0) => break,
                Ok(_) => {}
                Err(error) => {
                    let _ = session.take().abort().await;
                    return Err(DiaryError::AttachmentUpload(error.to_string()));
                }
            }
        }
        if chunk.is_empty() {
            break;
        }
        if upload_canceled(cancellation) {
            return Err(abort_canceled_upload(session).await);
        }

        let chunk_size = chunk.len() as u64;
        if let Err(error) = session.session_mut().write_chunk(chunk).await {
            let _ = session.take().abort().await;
            return Err(error);
        }
        if upload_canceled(cancellation) {
            return Err(abort_canceled_upload(session).await);
        }
        transferred = transferred.saturating_add(chunk_size);
        // 100% 只留给整个附件事务完成；传输结束后还要提交 multipart、
        // 固化本地缓存并更新日记 manifest。
        let percent = if expected_size == 0 {
            99
        } else {
            (transferred as u128 * 99 / expected_size as u128).min(99) as u8
        };
        progress(AttachmentUploadProgress::Transferring(percent));
    }

    if upload_canceled(cancellation) {
        return Err(abort_canceled_upload(session).await);
    }
    progress(AttachmentUploadProgress::Finalizing);
    if upload_canceled(cancellation) {
        return Err(abort_canceled_upload(session).await);
    }
    session.take().finish().await
}

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

async fn delete_local_diary_files(los: &LocalObjectStore, id: &str) -> Result<(), DiaryError> {
    let entries = match los.get_all().await {
        Ok(entries) => entries,
        // 尚未产生任何本地缓存时，LOS 目录可能还没有创建；删除应保持幂等。
        Err(CacheError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    let keys = entries.into_iter().map(|(key, _)| key);
    let (attachment_keys, manifest_key) = diary_object_keys(keys, id);

    // manifest 是日记是否存在的提交标志。只有附件全部删除成功后才删除它，
    // 这样失败时日记仍可见且可以安全重试。
    for key in attachment_keys {
        los.delete(&key).await?;
    }
    los.delete(&manifest_key).await?;
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
    /// 获取加密 manifest 对象的真实字节数。
    async fn get_manifest_size(&self, id: &str) -> Result<u64, DiaryError>;
    /// 删除日记（manifest + 所有附件）
    async fn delete_diary_all(&self, id: &str) -> Result<(), DiaryError>;
    /// 列出日记 ID（分页）
    async fn list_diary_ids(
        &self,
        next_token: NextToken,
    ) -> Result<(Vec<String>, NextToken), DiaryError>;
    /// 上传附件（有界流式），返回 etag
    #[cfg(test)]
    async fn upload_attachment(
        &self,
        id: &str,
        filename: &str,
        size: u64,
        mimetype: &str,
        stream: ByteStream,
    ) -> Result<String, DiaryError> {
        self.upload_attachment_with_progress(
            id,
            filename,
            size,
            mimetype,
            stream,
            AttachmentUploadOptions::new(Arc::new(|_| {}), None),
        )
        .await
    }
    /// 上传附件并在存储确认分片后报告进度。
    async fn upload_attachment_with_progress(
        &self,
        id: &str,
        filename: &str,
        size: u64,
        mimetype: &str,
        stream: ByteStream,
        options: AttachmentUploadOptions,
    ) -> Result<String, DiaryError> {
        let session = self
            .begin_attachment_upload(id, filename, size, mimetype)
            .await?;
        upload_stream_to_session(
            session,
            size,
            stream,
            options.progress,
            options.cancellation.as_ref(),
        )
        .await
    }
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
    /// 获取附件对象的真实字节数。Range/HEAD 响应不能依赖可能有误的历史 Manifest size。
    async fn get_attachment_size(
        &self,
        id: &str,
        filename: &str,
        known_etag: Option<&str>,
    ) -> Result<u64, DiaryError>;
    /// 将完整附件缓存到本地；已经命中有效缓存时直接成功。
    async fn cache_attachment(
        &self,
        id: &str,
        filename: &str,
        progress: StoreProgressCallback,
    ) -> Result<(), DiaryError>;
    /// 删除附件
    async fn delete_attachment(&self, id: &str, filename: &str) -> Result<(), DiaryError>;
    /// 覆盖附件前创建临时备份，供 Manifest 发布失败时回滚。
    async fn create_attachment_backup(&self, id: &str, filename: &str) -> Result<(), DiaryError>;
    /// 使用临时备份恢复附件。
    async fn restore_attachment_backup(
        &self,
        id: &str,
        filename: &str,
        mimetype: &str,
    ) -> Result<(), DiaryError>;
    /// 删除附件临时备份。
    async fn delete_attachment_backup(&self, id: &str, filename: &str) -> Result<(), DiaryError>;
}

// =============================================================================
// LocalStore — 仅使用 LocalObjectStore
// =============================================================================

pub struct LocalStore {
    los: LocalObjectStore,
}

impl LocalStore {
    pub fn new(los: LocalObjectStore) -> Self {
        Self { los }
    }
}

#[async_trait]
impl DiaryStore for LocalStore {
    async fn upload_manifest(&self, id: &str, data: &[u8]) -> Result<String, DiaryError> {
        let key = remote_manifest_key(id);
        self.los.save_bytes(&key, data).await?;
        let etag = format!("{:X}", md5::compute(data));
        Ok(etag)
    }

    async fn download_manifest(&self, id: &str) -> Result<(Vec<u8>, String), DiaryError> {
        let key = remote_manifest_key(id);
        let etag = self.los.get(&key).await?.ok_or(CacheError::NotFound)?;
        let data = self.los.get_data(&key).await?;
        Ok((data, etag))
    }

    async fn get_manifest_etag(&self, id: &str) -> Result<Option<String>, DiaryError> {
        let key = remote_manifest_key(id);
        Ok(self.los.get(&key).await?)
    }

    async fn get_manifest_size(&self, id: &str) -> Result<u64, DiaryError> {
        let key = remote_manifest_key(id);
        self.los
            .get_size(&key)
            .await?
            .ok_or(CacheError::NotFound.into())
    }

    async fn delete_diary_all(&self, id: &str) -> Result<(), DiaryError> {
        delete_local_diary_files(&self.los, id).await
    }

    async fn list_diary_ids(
        &self,
        next_token: NextToken,
    ) -> Result<(Vec<String>, NextToken), DiaryError> {
        let all = self.los.get_all().await?;
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

    async fn begin_attachment_upload(
        &self,
        id: &str,
        filename: &str,
        size: u64,
        _mimetype: &str,
    ) -> Result<Box<dyn AttachmentUploadSession>, DiaryError> {
        let key = remote_attachments_key(id, filename);
        Ok(Box::new(
            LocalAttachmentUpload::begin(self.los.clone(), key, size).await?,
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
        Ok(self.los.get_stream(&key, range).await?)
    }

    async fn get_attachment_size(
        &self,
        id: &str,
        filename: &str,
        _known_etag: Option<&str>,
    ) -> Result<u64, DiaryError> {
        let key = remote_attachments_key(id, filename);
        self.los
            .get_size(&key)
            .await?
            .ok_or_else(|| CacheError::NotFound.into())
    }

    async fn cache_attachment(
        &self,
        id: &str,
        filename: &str,
        progress: StoreProgressCallback,
    ) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        self.los.get(&key).await?.ok_or(CacheError::NotFound)?;
        progress(100);
        Ok(())
    }

    async fn delete_attachment(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        self.los.delete(&key).await?;
        Ok(())
    }

    async fn create_attachment_backup(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        let backup_key = attachment_backup_key(id, filename);
        let etag = self.los.get(&key).await?.ok_or(CacheError::NotFound)?;
        let stream = self.los.get_stream(&key, None).await?;
        self.los
            .save_stream_with_etag(&backup_key, &etag, stream)
            .await?;
        Ok(())
    }

    async fn restore_attachment_backup(
        &self,
        id: &str,
        filename: &str,
        _mimetype: &str,
    ) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        let backup_key = attachment_backup_key(id, filename);
        let etag = self
            .los
            .get(&backup_key)
            .await?
            .ok_or(CacheError::NotFound)?;
        let stream = self.los.get_stream(&backup_key, None).await?;
        self.los.save_stream_with_etag(&key, &etag, stream).await?;
        Ok(())
    }

    async fn delete_attachment_backup(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        self.los
            .delete(&attachment_backup_key(id, filename))
            .await?;
        Ok(())
    }
}

// =============================================================================
// RemoteStore — OSS + LOS 写透缓存
// =============================================================================

pub struct RemoteStore {
    los: LocalObjectStore,
    client: OssClient,
    attachment_cache: AttachmentCacheManager,
}

impl RemoteStore {
    #[cfg(test)]
    pub fn new(los: LocalObjectStore, client: OssClient) -> Self {
        let attachment_cache = AttachmentCacheManager::new(
            los.clone(),
            AppConfigStore::in_memory(AppConfig::default()),
        );
        Self::with_attachment_cache(los, client, attachment_cache)
    }

    pub fn with_attachment_cache(
        los: LocalObjectStore,
        client: OssClient,
        attachment_cache: AttachmentCacheManager,
    ) -> Self {
        Self {
            los,
            client,
            attachment_cache,
        }
    }

    async fn touch_cached_attachment(&self, key: &str) {
        if let Err(error) = self.attachment_cache.touch(key).await {
            tauri_plugin_log::log::warn!("更新附件缓存访问时间失败: key={key}, error={error}");
        }
    }

    async fn remove_cached_attachment(&self, key: &str) {
        if let Err(error) = self.los.delete(key).await {
            tauri_plugin_log::log::warn!("删除失效附件缓存失败: key={key}, error={error}");
        }
        if let Err(error) = self.attachment_cache.forget(key).await {
            tauri_plugin_log::log::warn!("移除附件缓存索引失败: key={key}, error={error}");
        }
    }
}

#[async_trait]
impl DiaryStore for RemoteStore {
    async fn upload_manifest(&self, id: &str, data: &[u8]) -> Result<String, DiaryError> {
        let key = remote_manifest_key(id);
        let etag = self.client.upload_bytes(&key, data).await?;
        let _ = self.los.save_bytes(&key, data).await;
        Ok(etag)
    }

    async fn download_manifest(&self, id: &str) -> Result<(Vec<u8>, String), DiaryError> {
        let key = remote_manifest_key(id);
        // 优先检查本地缓存
        if let Some(cached_etag) = self.los.get(&key).await? {
            let metadata = self.client.get_metadata(&key).await?;
            if metadata.etag.as_deref() == Some(&cached_etag) {
                let data = self.los.get_data(&key).await?;
                return Ok((data, cached_etag));
            }
        }
        // 缓存未命中，从 OSS 下载
        let data = self.client.download_bytes(&key).await?;
        let metadata = self.client.get_metadata(&key).await?;
        let etag = metadata.etag.unwrap_or_default();
        let _ = self.los.save_bytes(&key, &data).await;
        Ok((data, etag))
    }

    async fn get_manifest_etag(&self, id: &str) -> Result<Option<String>, DiaryError> {
        let key = remote_manifest_key(id);
        let metadata = self.client.get_metadata(&key).await?;
        Ok(metadata.etag)
    }

    async fn get_manifest_size(&self, id: &str) -> Result<u64, DiaryError> {
        let key = remote_manifest_key(id);
        self.client
            .get_metadata(&key)
            .await?
            .content_length
            .ok_or_else(|| {
                DiaryError::Object(ObjectError::OperationFailed(format!(
                    "日记主文件缺少 Content-Length: {key}"
                )))
            })
    }

    async fn delete_diary_all(&self, id: &str) -> Result<(), DiaryError> {
        let prefix = format!("{id}/");
        let keys = self.client.list_all_keys(&prefix).await?;
        let (attachment_keys, manifest_key) = diary_object_keys(keys, id);

        self.client.delete_keys(attachment_keys).await?;
        self.client.delete(&manifest_key).await?;

        // 远端是权威来源；远端完整删除后再清理本地副本。缓存清理失败也要
        // 返回错误，使用户可以重试，避免切换本地模式后旧日记重新出现。
        delete_local_diary_files(&self.los, id).await?;
        self.attachment_cache.forget_diary(id).await?;
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
                self.los.clone(),
                self.attachment_cache.clone(),
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
        if let Some(cached_etag) = self.los.get(&key).await? {
            // 已知 etag 匹配，直接使用缓存
            if known_etag.is_some_and(|k| k == cached_etag) {
                self.touch_cached_attachment(&key).await;
                return Ok(self.los.get_stream(&key, range).await?);
            }
            let metadata = self.client.get_metadata(&key).await?;
            if metadata.etag.as_deref() == Some(&cached_etag) {
                self.touch_cached_attachment(&key).await;
                return Ok(self.los.get_stream(&key, range).await?);
            } else {
                self.remove_cached_attachment(&key).await;
            }
        }
        let (stream, _) = self.client.download(&key, range).await?;
        Ok(stream)
    }

    async fn get_attachment_size(
        &self,
        id: &str,
        filename: &str,
        known_etag: Option<&str>,
    ) -> Result<u64, DiaryError> {
        let key = remote_attachments_key(id, filename);
        if let Some(cached_etag) = self.los.get(&key).await? {
            if known_etag.is_some_and(|etag| etags_match(etag, &cached_etag)) {
                if let Some(size) = self.los.get_size(&key).await? {
                    return Ok(size);
                }
            }

            let metadata = self.client.get_metadata(&key).await?;
            if metadata
                .etag
                .as_deref()
                .is_some_and(|etag| etags_match(etag, &cached_etag))
            {
                if let Some(size) = self.los.get_size(&key).await? {
                    return Ok(size);
                }
            } else {
                self.remove_cached_attachment(&key).await;
            }
            return metadata.content_length.ok_or_else(|| {
                DiaryError::Object(ObjectError::OperationFailed(format!(
                    "附件缺少 Content-Length: {key}"
                )))
            });
        }

        self.client
            .get_metadata(&key)
            .await?
            .content_length
            .ok_or_else(|| {
                DiaryError::Object(ObjectError::OperationFailed(format!(
                    "附件缺少 Content-Length: {key}"
                )))
            })
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
            .los
            .get(&key)
            .await?
            .is_some_and(|cached_etag| etags_match(&cached_etag, &remote_etag))
        {
            self.touch_cached_attachment(&key).await;
            progress(100);
            return Ok(());
        }

        let (stream, size) = self.client.download(&key, None).await?;
        self.attachment_cache.reserve(&key, size).await?;
        let progress_for_stream = progress.clone();
        let tracked_stream = tracker_stream(size, stream, move |percent| {
            progress_for_stream(percent);
        });
        if let Err(error) = self
            .los
            .save_stream_with_etag(&key, &remote_etag, tracked_stream)
            .await
        {
            self.attachment_cache.cancel_reservation(&key).await;
            return Err(error.into());
        }
        self.attachment_cache.commit(&key, size).await?;
        progress(100);
        Ok(())
    }

    async fn delete_attachment(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        self.client.delete(&key).await?;
        let _ = self.los.delete(&key).await;
        self.attachment_cache.forget(&key).await?;
        Ok(())
    }

    async fn create_attachment_backup(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        self.cache_attachment(id, filename, Arc::new(|_| {}))
            .await?;
        let key = remote_attachments_key(id, filename);
        let backup_key = attachment_backup_key(id, filename);
        let etag = self.los.get(&key).await?.ok_or(CacheError::NotFound)?;
        let stream = self.los.get_stream(&key, None).await?;
        self.los
            .save_stream_with_etag(&backup_key, &etag, stream)
            .await?;
        Ok(())
    }

    async fn restore_attachment_backup(
        &self,
        id: &str,
        filename: &str,
        mimetype: &str,
    ) -> Result<(), DiaryError> {
        let key = remote_attachments_key(id, filename);
        let backup_key = attachment_backup_key(id, filename);
        let size = self
            .los
            .get_size(&backup_key)
            .await?
            .ok_or(CacheError::NotFound)?;
        let stream = self.los.get_stream(&backup_key, None).await?;
        let remote_etag = self.client.upload(&key, size, stream, mimetype).await?;
        let cache_stream = self.los.get_stream(&backup_key, None).await?;
        self.los
            .save_stream_with_etag(&key, &remote_etag, cache_stream)
            .await?;
        if let Err(error) = self.attachment_cache.register_existing(&key).await {
            tauri_plugin_log::log::warn!(
                "远端附件恢复成功，但登记本地缓存失败: key={key}, error={error}"
            );
        }
        Ok(())
    }

    async fn delete_attachment_backup(&self, id: &str, filename: &str) -> Result<(), DiaryError> {
        self.los
            .delete(&attachment_backup_key(id, filename))
            .await?;
        Ok(())
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
    use crate::cryptos::Crypto;
    use crate::diaries::diary::{
        delete_diary, get_diary, lock_diary_operation, save_diary, update_diary_content_only,
    };
    use crate::stream::create_mock_stream;
    use crate::utils::id_generate::generate_descending_id_with_timestamp;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::StreamExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use tokio::sync::oneshot;
    use tokio::time::{timeout, Duration};

    struct RecordingUploadSession {
        aborted: Arc<AtomicUsize>,
        first_write: Option<oneshot::Sender<()>>,
    }

    #[async_trait]
    impl AttachmentUploadSession for RecordingUploadSession {
        async fn write_chunk(&mut self, _data: Vec<u8>) -> Result<(u32, String), DiaryError> {
            if let Some(first_write) = self.first_write.take() {
                let _ = first_write.send(());
            }
            Ok((1, String::new()))
        }

        async fn finish(self: Box<Self>) -> Result<String, DiaryError> {
            Ok(String::new())
        }

        async fn abort(self: Box<Self>) -> Result<(), DiaryError> {
            self.aborted.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

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

    fn make_los() -> (LocalObjectStore, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        (
            LocalObjectStore::new(temp_dir.path().to_path_buf()),
            temp_dir,
        )
    }

    fn make_local_store() -> (LocalStore, LocalObjectStore, tempfile::TempDir) {
        let (los, td) = make_los();
        (LocalStore::new(los.clone()), los, td)
    }

    #[tokio::test]
    async fn dropping_upload_future_still_aborts_its_session() {
        let aborted = Arc::new(AtomicUsize::new(0));
        let (first_write_tx, first_write_rx) = oneshot::channel();
        let session = RecordingUploadSession {
            aborted: aborted.clone(),
            first_write: Some(first_write_tx),
        };
        let first_chunk = futures_util::stream::once(async {
            Ok(Bytes::from(vec![0_u8; ATTACHMENT_UPLOAD_CHUNK_SIZE]))
        });
        let pending_tail = futures_util::stream::pending();
        let stream: ByteStream = Box::pin(first_chunk.chain(pending_tail));
        let upload = tokio::spawn(upload_stream_to_session(
            Box::new(session),
            ATTACHMENT_UPLOAD_CHUNK_SIZE as u64 + 1,
            stream,
            Arc::new(|_| {}),
            None,
        ));

        first_write_rx.await.expect("首个分片未写入");
        upload.abort();
        let _ = upload.await;

        timeout(Duration::from_secs(1), async {
            while aborted.load(Ordering::Relaxed) != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Future 丢弃后未执行上传会话清理");
    }

    // ==================== LocalStore 基础 CRUD ====================

    #[tokio::test]
    async fn test_local_store_manifest_roundtrip() {
        let (store, _los, _td) = make_local_store();
        let data = b"encrypted manifest data";

        let etag = store.upload_manifest("test-id-1", data).await.unwrap();
        assert!(!etag.is_empty());

        let (downloaded, returned_etag) = store.download_manifest("test-id-1").await.unwrap();
        assert_eq!(downloaded, data);
        assert_eq!(returned_etag, etag);
        assert_eq!(
            store.get_manifest_size("test-id-1").await.unwrap(),
            data.len() as u64
        );
    }

    #[tokio::test]
    async fn test_local_store_get_missing_manifest_size() {
        let (store, _los, _td) = make_local_store();

        assert!(store.get_manifest_size("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_local_store_get_manifest_etag() {
        let (store, _los, _td) = make_local_store();

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
        let (store, _los, _td) = make_local_store();

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
    async fn local_delete_is_idempotent_without_store_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp_dir.path().join("missing"));
        let store = LocalStore::new(los);

        store.delete_diary_all("missing-diary").await.unwrap();
    }

    #[tokio::test]
    async fn local_delete_propagates_cache_scan_errors() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_file = temp_dir.path().join("not-a-directory");
        tokio::fs::write(&cache_file, b"file").await.unwrap();
        let store = LocalStore::new(LocalObjectStore::new(cache_file));

        assert!(matches!(
            store.delete_diary_all("diary").await,
            Err(DiaryError::Cache(CacheError::Io(_)))
        ));
    }

    #[tokio::test]
    async fn test_local_store_list_diary_ids() {
        let (store, _los, _td) = make_local_store();

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
        let (store, _los, _td) = make_local_store();

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
        let (store, _los, _td) = make_local_store();
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
        let (store, _los, _td) = make_local_store();
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
        let (store, _los, _td) = make_local_store();

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

    // ==================== LocalStore + 日记函数集成 ====================

    #[tokio::test]
    async fn test_local_store_save_and_get_diary() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _los, _td) = make_local_store();

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
    async fn test_local_store_rejects_legacy_manifest_without_rewriting_it() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _los, _td) = make_local_store();
        let diary_id = "legacy-v3";
        let legacy = serde_json::json!({"id": diary_id, "version": 3});
        let encrypted = crypto
            .encrypt(&serde_json::to_vec(&legacy).unwrap())
            .unwrap();
        let original_etag = store.upload_manifest(diary_id, &encrypted).await.unwrap();

        assert!(matches!(
            get_diary(&cache, &crypto, &store, diary_id).await,
            Err(DiaryError::UnsupportedVersion {
                found: 3,
                supported: 4
            })
        ));

        let (stored, stored_etag) = store.download_manifest(diary_id).await.unwrap();
        assert_eq!(stored, encrypted);
        assert_eq!(stored_etag, original_etag);
    }

    #[tokio::test]
    async fn test_local_store_update_diary() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _los, _td) = make_local_store();

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
        let (store, _los, _td) = make_local_store();

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
    async fn delete_waits_for_an_active_diary_operation() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, los, _td) = make_local_store();
        let (summary, _) = save_diary(&cache, &crypto, &store, "serialized delete")
            .await
            .unwrap();
        let guard = lock_diary_operation(&summary.id).await;

        let (started_tx, started_rx) = oneshot::channel();
        let task_cache = cache.clone();
        let task_id = summary.id.clone();
        let deletion = tokio::spawn(async move {
            let _ = started_tx.send(());
            let task_store = LocalStore::new(los);
            delete_diary(&task_cache, &task_store, &task_id).await
        });

        started_rx.await.unwrap();
        tokio::task::yield_now().await;
        assert!(!deletion.is_finished(), "删除不应越过日记操作锁");

        drop(guard);
        deletion.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_local_store_list_after_save() {
        let cache = DiaryMemoryCache::new();
        let crypto = make_crypto();
        let (store, _los, _td) = make_local_store();

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
        let (store, _los, _td) = make_local_store();

        let result = get_diary(&cache, &crypto, &store, "nonexistent-id").await;
        assert!(result.is_err());
    }

    // ==================== AppState.diary_store() 行为 ====================

    #[test]
    fn test_app_state_diary_store_local_mode() {
        let (los, _td) = make_los();
        let state = crate::state::AppState::from_parts(Crypto::new(), OssClient::new(), los);
        // 默认 remote_enabled = false
        assert!(!state.is_remote_enabled());
    }

    #[test]
    fn test_app_state_diary_store_remote_mode() {
        let (los, _td) = make_los();
        let state = crate::state::AppState::from_parts(Crypto::new(), OssClient::new(), los);
        state.set_remote_enabled(true);
        assert!(state.is_remote_enabled());

        state.set_remote_enabled(false);
        assert!(!state.is_remote_enabled());
    }

    #[tokio::test]
    async fn test_local_store_data_persistence_across_instances() {
        let (los, _td) = make_los();

        // 用第一个 store 实例写入
        {
            let store = LocalStore::new(los.clone());
            store
                .upload_manifest("persist-test", b"persistent data")
                .await
                .unwrap();
        }

        // 用第二个 store 实例读取（模拟应用重启）
        {
            let store = LocalStore::new(los.clone());
            let (data, _etag) = store.download_manifest("persist-test").await.unwrap();
            assert_eq!(data, b"persistent data");
        }
    }

    // ==================== 边界情况 ====================

    #[tokio::test]
    async fn test_local_store_empty_manifest() {
        let (store, _los, _td) = make_local_store();
        let etag = store.upload_manifest("empty", b"").await.unwrap();
        let (data, _) = store.download_manifest("empty").await.unwrap();
        assert!(data.is_empty());
        assert!(!etag.is_empty());
    }

    #[tokio::test]
    async fn test_local_store_overwrite_manifest() {
        let (store, _los, _td) = make_local_store();

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
        let (store, _los, _td) = make_local_store();
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
        let (los, _td) = make_los();
        let local_store = LocalStore::new(los.clone());

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
