use super::progress::FileProgress;
use super::types::Entry;
use super::{error, Result};
use crate::object_locations::ObjectLocations;
use crate::state::AppState;
use crate::stream::ByteStream;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use tokio_util::io::ReaderStream;

#[async_trait]
pub(super) trait RekeyStorage: Send + Sync {
    fn diagnostics(&self) -> Option<super::diagnostics::Diagnostics> {
        None
    }
    async fn list(&self) -> Result<Vec<Entry>>;
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<Entry>> {
        Ok(self
            .list()
            .await?
            .into_iter()
            .filter(|e| e.key.starts_with(prefix))
            .collect())
    }
    async fn read(&self, key: &str) -> Result<ByteStream>;
    async fn head(&self, key: &str) -> Result<Option<Entry>>;
    async fn write(&self, key: &str, size: u64, stream: ByteStream) -> Result<String>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn checkpoint(&self, journal: &str) -> Result<()>;
    async fn copy(
        &self,
        source: &str,
        target: &str,
        size: u64,
        progress: &FileProgress,
    ) -> Result<String> {
        self.write(target, size, progress.wrap(self.read(source).await?))
            .await
    }
}

pub(super) struct Storage {
    pub state: AppState,
    pub remote: bool,
}

#[async_trait]
impl RekeyStorage for Storage {
    async fn list(&self) -> Result<Vec<Entry>> {
        self.list_prefix("").await
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<Entry>> {
        if !self.remote {
            return Ok(self
                .state
                .local_object_store()
                .get_all_entries()
                .await
                .map_err(error)?
                .into_iter()
                .map(|entry| Entry {
                    key: entry.key,
                    size: entry.size,
                    etag: entry.etag,
                })
                .filter(|entry| entry.key.starts_with(prefix))
                .collect());
        }
        let mut entries = vec![];
        let mut token = None;
        loop {
            let (page, next) = self
                .state
                .oss_client()
                .list(prefix, token)
                .await
                .map_err(error)?;
            for entry in page {
                entries.push(Entry {
                    key: entry.key,
                    size: entry.size,
                    etag: entry
                        .etag
                        .ok_or_else(|| error("对象没有 ETag，无法安全改密"))?,
                });
            }
            token = next;
            if token.is_none() {
                break;
            }
        }
        Ok(entries)
    }

    async fn read(&self, key: &str) -> Result<ByteStream> {
        if self.remote {
            Ok(self
                .state
                .oss_client()
                .download(key, None)
                .await
                .map_err(error)?
                .0)
        } else {
            // 恢复时目标可能已换成新密文但 ETag 文件尚未落盘，仍须能校验数据文件。
            let path = self
                .state
                .local_object_store()
                .root()
                .join(format!("{key}.data"));
            Ok(Box::pin(ReaderStream::new(
                tokio::fs::File::open(path).await.map_err(error)?,
            )))
        }
    }

    async fn head(&self, key: &str) -> Result<Option<Entry>> {
        if self.remote {
            if !self
                .state
                .oss_client()
                .object_exists(key)
                .await
                .map_err(error)?
            {
                return Ok(None);
            }
            let metadata = self
                .state
                .oss_client()
                .get_metadata(key)
                .await
                .map_err(error)?;
            Ok(Some(Entry {
                key: key.into(),
                size: metadata
                    .content_length
                    .ok_or_else(|| error("对象缺少大小"))?,
                etag: metadata.etag.ok_or_else(|| error("对象缺少 ETag"))?,
            }))
        } else {
            let los = self.state.local_object_store();
            let path = los.root().join(format!("{key}.data"));
            if !tokio::fs::try_exists(&path).await.map_err(error)? {
                return Ok(None);
            }
            Ok(Some(Entry {
                key: key.into(),
                size: tokio::fs::metadata(path).await.map_err(error)?.len(),
                etag: los.get(key).await.map_err(error)?.unwrap_or_default(),
            }))
        }
    }

    async fn write(&self, key: &str, size: u64, stream: ByteStream) -> Result<String> {
        if self.remote {
            // 重加密的新密文仍流式上传；使用串行 multipart 保持有界内存。
            self.state
                .oss_client()
                .upload(key, size, stream, "application/octet-stream")
                .await
                .map_err(error)
        } else {
            let los = self.state.local_object_store();
            let handle = los.begin_chunked_save(key).await.map_err(error)?;
            let result = async {
                let mut stream = stream;
                let mut transferred = 0_u64;
                let mut md5 = md5::Context::new();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(error)?;
                    transferred = transferred
                        .checked_add(chunk.len() as u64)
                        .ok_or_else(|| error("对象大小溢出"))?;
                    if transferred > size {
                        return Err(error("写入流超过预计大小"));
                    }
                    md5.consume(&chunk);
                    handle.write_chunk(&chunk).await.map_err(error)?;
                }
                if transferred != size {
                    return Err(error("写入流大小不匹配"));
                }
                Ok(format!("{:X}", md5.finalize()))
            }
            .await;
            match result {
                Ok(etag) => {
                    handle.finalize(&etag).await.map_err(error)?;
                    // 数据已 sync_all；ETag 和 Android 目录项也须在检查点前持久保存。
                    let etag_path = los.root().join(format!("{key}.md5"));
                    tokio::fs::OpenOptions::new()
                        .write(true)
                        .open(&etag_path)
                        .await
                        .map_err(error)?
                        .sync_all()
                        .await
                        .map_err(error)?;
                    #[cfg(unix)]
                    tokio::fs::File::open(etag_path.parent().ok_or_else(|| error("对象目录无效"))?)
                        .await
                        .map_err(error)?
                        .sync_all()
                        .await
                        .map_err(error)?;
                    Ok(etag)
                }
                Err(e) => {
                    handle.abort().await;
                    Err(e)
                }
            }
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        if self.remote {
            self.state.oss_client().delete(key).await.map_err(error)
        } else {
            self.state
                .local_object_store()
                .delete(key)
                .await
                .map_err(error)
        }
    }

    async fn checkpoint(&self, journal: &str) -> Result<()> {
        // 两份记录均成功后才能替换下一个对象；本机领先一步也不会丢失云端恢复资料。
        self.state
            .persist_key_change(Some(journal.into()))
            .map_err(error)?;
        if self.remote {
            self.state
                .oss_client()
                .upload_bytes(ObjectLocations::vault_key_change(), journal.as_bytes())
                .await
                .map_err(error)?;
        }
        Ok(())
    }

    async fn copy(
        &self,
        source: &str,
        target: &str,
        size: u64,
        progress: &FileProgress,
    ) -> Result<String> {
        if self.remote {
            // OSS 同桶复制且不请求变更服务端加密/存储类型时支持大对象（包括 > 5 GB）。
            // 这里复制的是应用已经加密的字节，不涉及 OSS 的服务端加密转换。
            // https://help.aliyun.com/zh/oss/developer-reference/copyobject
            // 不静默退回下载重传；复制失败仍保留检查点和备份，供用户续改。
            self.state
                .oss_client()
                .copy_object(source, target)
                .await
                .map_err(error)?;
            let copied = self
                .head(target)
                .await?
                .ok_or_else(|| error("复制的对象不存在"))?;
            if copied.size != size {
                return Err(error("云端复制的对象大小不匹配"));
            }
            return Ok(copied.etag);
        }
        // 本地流式复制可计数字节；云内复制只报告开始/完成，不伪造中间进度。
        self.write(target, size, progress.wrap(self.read(source).await?))
            .await
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

pub(super) fn byte_stream(bytes: Vec<u8>) -> ByteStream {
    Box::pin(futures_util::stream::once(
        async move { Ok(Bytes::from(bytes)) },
    ))
}

pub(super) async fn read_limited(
    storage: &dyn RekeyStorage,
    key: &str,
    limit: usize,
) -> Result<Vec<u8>> {
    read_limited_stream(storage.read(key).await?, key, limit).await
}

pub(super) async fn read_limited_stream(
    mut stream: ByteStream,
    key: &str,
    limit: usize,
) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(error)?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(error(format!("对象 {key} 超过安全读取上限")));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
