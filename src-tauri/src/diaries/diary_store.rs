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
