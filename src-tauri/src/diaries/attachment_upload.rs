use async_trait::async_trait;

use crate::caches::{ChunkedSaveHandle, LocalFileCache};
use crate::diaries::DiaryError;
use crate::object::{ObjectError, OssClient};

#[async_trait]
pub trait AttachmentUploadSession: Send {
    async fn write_chunk(&mut self, data: Vec<u8>) -> Result<(u32, String), DiaryError>;
    async fn finish(self: Box<Self>) -> Result<String, DiaryError>;
    async fn abort(self: Box<Self>) -> Result<(), DiaryError>;
}

fn invalid_upload(message: impl Into<String>) -> DiaryError {
    DiaryError::AttachmentUpload(message.into())
}

pub(crate) struct LocalAttachmentUpload {
    lfc: LocalFileCache,
    key: String,
    handle: Option<ChunkedSaveHandle>,
    digest: md5::Context,
    expected_size: u64,
    written_size: u64,
    next_part_number: u32,
}

impl LocalAttachmentUpload {
    pub(crate) async fn begin(
        lfc: LocalFileCache,
        key: String,
        expected_size: u64,
    ) -> Result<Self, DiaryError> {
        let handle = lfc.begin_chunked_save(&key).await?;
        Ok(Self {
            lfc,
            key,
            handle: Some(handle),
            digest: md5::Context::new(),
            expected_size,
            written_size: 0,
            next_part_number: 1,
        })
    }

    async fn abort_inner(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort().await;
        }
    }

    fn checked_written_size(&self, chunk_size: usize) -> Result<u64, DiaryError> {
        let written_size = self
            .written_size
            .checked_add(chunk_size as u64)
            .ok_or_else(|| invalid_upload("附件分片累计大小溢出"))?;
        if written_size > self.expected_size {
            return Err(invalid_upload(format!(
                "附件分片超过声明大小：expected={}, actual={written_size}",
                self.expected_size
            )));
        }
        Ok(written_size)
    }
}

#[async_trait]
impl AttachmentUploadSession for LocalAttachmentUpload {
    async fn write_chunk(&mut self, data: Vec<u8>) -> Result<(u32, String), DiaryError> {
        let written_size = match self.checked_written_size(data.len()) {
            Ok(size) => size,
            Err(error) => {
                self.abort_inner().await;
                return Err(error);
            }
        };
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| invalid_upload("附件上传会话已经结束"))?;
        if let Err(error) = handle.write_chunk(&data).await {
            self.abort_inner().await;
            return Err(error.into());
        }

        self.digest.consume(&data);
        self.written_size = written_size;
        let part_number = self.next_part_number;
        self.next_part_number += 1;
        Ok((part_number, String::new()))
    }

    async fn finish(mut self: Box<Self>) -> Result<String, DiaryError> {
        if self.written_size != self.expected_size {
            self.abort_inner().await;
            return Err(invalid_upload(format!(
                "附件分片大小不完整：expected={}, actual={}",
                self.expected_size, self.written_size
            )));
        }

        let etag = format!("{:X}", self.digest.clone().finalize());
        let handle = self
            .handle
            .take()
            .ok_or_else(|| invalid_upload("附件上传会话已经结束"))?;
        if let Err(error) = handle.finalize(&etag).await {
            let _ = self.lfc.delete(&self.key).await;
            return Err(error.into());
        }
        Ok(etag)
    }

    async fn abort(mut self: Box<Self>) -> Result<(), DiaryError> {
        self.abort_inner().await;
        Ok(())
    }
}

pub(crate) struct RemoteAttachmentUpload {
    lfc: LocalFileCache,
    client: OssClient,
    key: String,
    mimetype: String,
    upload_id: String,
    handle: Option<ChunkedSaveHandle>,
    parts: Vec<(String, u32)>,
    expected_size: u64,
    written_size: u64,
    next_part_number: u32,
    multipart_active: bool,
}

impl RemoteAttachmentUpload {
    pub(crate) async fn begin(
        lfc: LocalFileCache,
        client: OssClient,
        key: String,
        expected_size: u64,
        mimetype: String,
    ) -> Result<Self, DiaryError> {
        let upload_id = client.initiate_multipart_upload(&key, &mimetype).await?;
        let handle = match lfc.begin_chunked_save(&key).await {
            Ok(handle) => handle,
            Err(error) => {
                let _ = client.abort_multipart_upload(&key, &upload_id).await;
                return Err(error.into());
            }
        };
        Ok(Self {
            lfc,
            client,
            key,
            mimetype,
            upload_id,
            handle: Some(handle),
            parts: Vec::new(),
            expected_size,
            written_size: 0,
            next_part_number: 1,
            multipart_active: true,
        })
    }

    async fn abort_inner(&mut self) -> Result<(), DiaryError> {
        if let Some(handle) = self.handle.take() {
            handle.abort().await;
        }
        if self.multipart_active {
            self.multipart_active = false;
            self.client
                .abort_multipart_upload(&self.key, &self.upload_id)
                .await?;
        }
        Ok(())
    }

    async fn abort_with_error(&mut self, primary: DiaryError) -> DiaryError {
        match self.abort_inner().await {
            Ok(()) => primary,
            Err(abort_error) => {
                invalid_upload(format!("{primary}；取消远端 multipart 失败：{abort_error}"))
            }
        }
    }

    fn checked_written_size(&self, chunk_size: usize) -> Result<u64, DiaryError> {
        let written_size = self
            .written_size
            .checked_add(chunk_size as u64)
            .ok_or_else(|| invalid_upload("附件分片累计大小溢出"))?;
        if written_size > self.expected_size {
            return Err(invalid_upload(format!(
                "附件分片超过声明大小：expected={}, actual={written_size}",
                self.expected_size
            )));
        }
        Ok(written_size)
    }
}

#[async_trait]
impl AttachmentUploadSession for RemoteAttachmentUpload {
    async fn write_chunk(&mut self, data: Vec<u8>) -> Result<(u32, String), DiaryError> {
        let written_size = match self.checked_written_size(data.len()) {
            Ok(size) => size,
            Err(error) => {
                return Err(self.abort_with_error(error).await);
            }
        };
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| invalid_upload("附件上传会话已经结束"))?;
        if let Err(error) = handle.write_chunk(&data).await {
            return Err(self.abort_with_error(error.into()).await);
        }

        let part_number = self.next_part_number;
        let (etag, returned_part_number) = match self
            .client
            .upload_part(
                &self.key,
                part_number,
                &self.upload_id,
                data,
                &self.mimetype,
            )
            .await
        {
            Ok(result) => result,
            Err(error) => {
                return Err(self.abort_with_error(error.into()).await);
            }
        };

        if returned_part_number != part_number {
            let error = invalid_upload(format!(
                "对象存储返回了错误的分片编号：expected={part_number}, actual={returned_part_number}"
            ));
            return Err(self.abort_with_error(error).await);
        }

        self.parts.push((etag.clone(), part_number));
        self.written_size = written_size;
        self.next_part_number += 1;
        Ok((part_number, etag))
    }

    async fn finish(mut self: Box<Self>) -> Result<String, DiaryError> {
        if self.written_size != self.expected_size {
            let error = invalid_upload(format!(
                "附件分片大小不完整：expected={}, actual={}",
                self.expected_size, self.written_size
            ));
            return Err(self.abort_with_error(error).await);
        }

        let parts = std::mem::take(&mut self.parts);
        let etag = match self
            .client
            .complete_multipart_upload(&self.key, &self.upload_id, parts)
            .await
        {
            Ok(etag) => {
                self.multipart_active = false;
                etag
            }
            Err(error) => {
                return Err(self.abort_with_error(error.into()).await);
            }
        };

        let handle = self
            .handle
            .take()
            .ok_or_else(|| invalid_upload("附件上传会话已经结束"))?;
        if let Err(cache_error) = handle.finalize(&etag).await {
            let local_rollback = self.lfc.delete(&self.key).await;
            let remote_rollback = self.client.delete(&self.key).await;
            if let Err(rollback_error) = remote_rollback {
                return Err(DiaryError::Object(ObjectError::OperationFailed(format!(
                    "本地附件固化失败：{cache_error}；远端回滚失败：{rollback_error}"
                ))));
            }
            local_rollback?;
            return Err(cache_error.into());
        }
        Ok(etag)
    }

    async fn abort(mut self: Box<Self>) -> Result<(), DiaryError> {
        self.abort_inner().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diaries::diary_store::{AttachmentUploadOptions, AttachmentUploadProgress};
    use crate::diaries::{DiaryStore, LocalStore, RemoteStore};
    use crate::stream::ByteStream;
    use crate::test_utils::{wait_for_multipart_upload_count, TestOssGuard};
    use bytes::Bytes;
    use futures_util::StreamExt;
    use std::io;
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;
    use tokio::time::{timeout, Duration};
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn local_session_writes_chunks_and_commits_real_etag() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = LocalStore::new(lfc.clone());
        let data = b"hello chunked world";
        let mut session = store
            .begin_attachment_upload(
                "diary",
                "attachment",
                data.len() as u64,
                "application/octet-stream",
            )
            .await
            .unwrap();

        assert_eq!(
            session.write_chunk(b"hello ".to_vec()).await.unwrap(),
            (1, String::new())
        );
        assert_eq!(
            session
                .write_chunk(b"chunked world".to_vec())
                .await
                .unwrap(),
            (2, String::new())
        );
        let etag = session.finish().await.unwrap();

        assert_eq!(etag, format!("{:X}", md5::compute(data)));
        assert_eq!(lfc.get("diary/attachment").await.unwrap(), Some(etag));
        assert_eq!(lfc.get_data("diary/attachment").await.unwrap(), data);
    }

    #[tokio::test]
    async fn local_session_rejects_incomplete_upload_and_removes_temp_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = LocalStore::new(lfc.clone());
        let mut session = store
            .begin_attachment_upload("diary", "attachment", 5, "application/octet-stream")
            .await
            .unwrap();
        session.write_chunk(b"123".to_vec()).await.unwrap();

        assert!(session.finish().await.is_err());
        assert!(lfc.get("diary/attachment").await.unwrap().is_none());
        assert!(std::fs::read_dir(temp_dir.path().join("diary"))
            .unwrap()
            .next()
            .is_none());
    }

    #[tokio::test]
    async fn local_session_abort_removes_partial_upload() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = LocalStore::new(lfc.clone());
        let mut session = store
            .begin_attachment_upload("diary", "attachment", 5, "application/octet-stream")
            .await
            .unwrap();
        session.write_chunk(b"123".to_vec()).await.unwrap();

        session.abort().await.unwrap();

        assert!(lfc.get("diary/attachment").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn local_session_rejects_data_larger_than_declared_size() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = LocalStore::new(lfc.clone());
        let mut session = store
            .begin_attachment_upload("diary", "attachment", 2, "application/octet-stream")
            .await
            .unwrap();

        assert!(session.write_chunk(b"123".to_vec()).await.is_err());
        assert!(session.abort().await.is_ok());
        assert!(lfc.get("diary/attachment").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn local_store_uploads_large_stream_with_confirmed_progress() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = LocalStore::new(lfc.clone());
        let data = vec![7_u8; 8 * 1024 * 1024 + 123];
        let progress_values = Arc::new(Mutex::new(Vec::new()));
        let captured_progress = progress_values.clone();

        let etag = store
            .upload_attachment_with_progress(
                "diary",
                "attachment",
                data.len() as u64,
                "application/octet-stream",
                crate::stream::create_mock_stream(data.clone(), 64 * 1024),
                AttachmentUploadOptions::new(
                    Arc::new(move |progress| captured_progress.lock().unwrap().push(progress)),
                    None,
                ),
            )
            .await
            .unwrap();

        assert_eq!(etag, format!("{:X}", md5::compute(&data)));
        assert_eq!(lfc.get_data("diary/attachment").await.unwrap(), data);
        let progress = progress_values.lock().unwrap();
        assert!(matches!(
            progress.last(),
            Some(AttachmentUploadProgress::Finalizing)
        ));
        assert!(progress
            .iter()
            .any(|value| matches!(value, AttachmentUploadProgress::Transferring(99))));
    }

    #[tokio::test]
    async fn local_store_aborts_partial_file_when_source_stream_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = LocalStore::new(lfc.clone());
        let stream: ByteStream = Box::pin(futures_util::stream::iter(vec![
            Ok(Bytes::from_static(b"partial")),
            Err(io::Error::other("simulated source failure")),
        ]));

        assert!(store
            .upload_attachment(
                "diary",
                "attachment",
                10,
                "application/octet-stream",
                stream,
            )
            .await
            .is_err());
        assert!(lfc.get("diary/attachment").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn local_store_cancellation_aborts_partial_file_after_confirmed_chunk() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = LocalStore::new(lfc.clone());
        let data = vec![7_u8; 8 * 1024 * 1024 + 123];
        let cancellation = CancellationToken::new();
        let cancellation_on_progress = cancellation.clone();

        let result = store
            .upload_attachment_with_progress(
                "diary",
                "attachment",
                data.len() as u64,
                "application/octet-stream",
                crate::stream::create_mock_stream(data, 64 * 1024),
                AttachmentUploadOptions::new(
                    Arc::new(move |progress| {
                        if matches!(progress, AttachmentUploadProgress::Transferring(_)) {
                            cancellation_on_progress.cancel();
                        }
                    }),
                    Some(cancellation.clone()),
                ),
            )
            .await;

        assert!(matches!(
            result,
            Err(DiaryError::AttachmentUpload(message)) if message.contains("已取消")
        ));
        assert!(lfc.get("diary/attachment").await.unwrap().is_none());
        assert!(std::fs::read_dir(temp_dir.path().join("diary"))
            .unwrap()
            .next()
            .is_none());
    }

    #[tokio::test]
    async fn local_store_cancellation_before_finalize_does_not_commit_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = LocalStore::new(lfc.clone());
        let cancellation = CancellationToken::new();
        let cancellation_on_progress = cancellation.clone();

        let result = store
            .upload_attachment_with_progress(
                "diary",
                "attachment",
                3,
                "application/octet-stream",
                crate::stream::create_mock_stream(b"123".to_vec(), 3),
                AttachmentUploadOptions::new(
                    Arc::new(move |progress| {
                        if progress == AttachmentUploadProgress::Finalizing {
                            cancellation_on_progress.cancel();
                        }
                    }),
                    Some(cancellation.clone()),
                ),
            )
            .await;

        assert!(matches!(
            result,
            Err(DiaryError::AttachmentUpload(message)) if message.contains("已取消")
        ));
        assert!(lfc.get("diary/attachment").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn remote_session_commits_same_data_and_etag_to_oss_and_cache() {
        let client = OssClient::from_env();
        let (client, guard) = TestOssGuard::new(client).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = RemoteStore::new(lfc.clone(), client.clone());
        let first_chunk = vec![7_u8; 5 * 1024 * 1024];
        let second_chunk = b"last chunk".to_vec();
        let total_size = (first_chunk.len() + second_chunk.len()) as u64;
        let mut session = store
            .begin_attachment_upload(
                "diary",
                "attachment",
                total_size,
                "application/octet-stream",
            )
            .await
            .unwrap();

        assert_eq!(session.write_chunk(first_chunk).await.unwrap().0, 1);
        assert_eq!(session.write_chunk(second_chunk).await.unwrap().0, 2);
        let etag = session.finish().await.unwrap();

        let metadata = client.get_metadata("diary/attachment").await.unwrap();
        assert_eq!(metadata.content_length, Some(total_size));
        assert_eq!(metadata.etag.as_deref(), Some(etag.as_str()));
        assert_eq!(
            lfc.get("diary/attachment").await.unwrap().as_deref(),
            Some(etag.as_str())
        );
        assert_eq!(
            lfc.get_data("diary/attachment").await.unwrap().len() as u64,
            total_size
        );
        guard.cleanup().await;
    }

    #[tokio::test]
    async fn remote_session_abort_removes_local_temp_and_remote_upload() {
        let client = OssClient::from_env();
        let (client, guard) = TestOssGuard::new(client).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = RemoteStore::new(lfc.clone(), client.clone());
        let mut session = store
            .begin_attachment_upload("diary", "attachment", 3, "application/octet-stream")
            .await
            .unwrap();
        session.write_chunk(b"123".to_vec()).await.unwrap();
        let uploads = wait_for_multipart_upload_count(&client, 1).await;
        assert_eq!(uploads[0].0, "diary/attachment");

        session.abort().await.unwrap();

        wait_for_multipart_upload_count(&client, 0).await;
        assert!(lfc.get("diary/attachment").await.unwrap().is_none());
        let (objects, _) = client.list("", None).await.unwrap();
        assert!(objects.is_empty());
        guard.cleanup().await;
    }

    #[tokio::test]
    async fn remote_store_task_cancellation_aborts_visible_multipart_upload() {
        let client = OssClient::from_env();
        let (client, guard) = TestOssGuard::new(client).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let lfc = LocalFileCache::new(temp_dir.path().to_path_buf());
        let store = RemoteStore::new(lfc.clone(), client.clone());
        let cancellation = CancellationToken::new();
        let cancellation_in_task = cancellation.clone();
        let (first_part_tx, first_part_rx) = oneshot::channel();
        let first_part_tx = Arc::new(Mutex::new(Some(first_part_tx)));
        let first_part_tx_on_progress = first_part_tx.clone();
        let first_chunk = futures_util::stream::once(async {
            Ok::<Bytes, io::Error>(Bytes::from(vec![7_u8; 8 * 1024 * 1024]))
        });
        let pending_tail = futures_util::stream::pending();
        let stream: ByteStream = Box::pin(first_chunk.chain(pending_tail));

        let upload = tokio::spawn(async move {
            store
                .upload_attachment_with_progress(
                    "diary",
                    "attachment",
                    8 * 1024 * 1024 + 1,
                    "application/octet-stream",
                    stream,
                    AttachmentUploadOptions::new(
                        Arc::new(move |progress| {
                            if matches!(progress, AttachmentUploadProgress::Transferring(_)) {
                                if let Some(sender) =
                                    first_part_tx_on_progress.lock().unwrap().take()
                                {
                                    let _ = sender.send(());
                                }
                            }
                        }),
                        Some(cancellation_in_task),
                    ),
                )
                .await
        });

        timeout(Duration::from_secs(30), first_part_rx)
            .await
            .expect("首个远端分片上传超时")
            .expect("上传任务未报告首个远端分片");
        let uploads = wait_for_multipart_upload_count(&client, 1).await;
        assert_eq!(uploads[0].0, "diary/attachment");

        cancellation.cancel();
        let result = timeout(Duration::from_secs(30), upload)
            .await
            .expect("取消远端上传超时")
            .expect("远端上传任务 panic");
        assert!(matches!(
            result,
            Err(DiaryError::AttachmentUpload(message)) if message.contains("已取消")
        ));
        wait_for_multipart_upload_count(&client, 0).await;
        assert!(lfc.get("diary/attachment").await.unwrap().is_none());
        let (objects, _) = client.list("", None).await.unwrap();
        assert!(objects.is_empty());
        guard.cleanup().await;
    }
}
