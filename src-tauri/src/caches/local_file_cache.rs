use crate::caches::CacheError;
use crate::stream::ByteStream;
use futures_util::StreamExt;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;

pub const LOCAL_FILE_CACHE_FILENAME: &str = "lfc";

const DATA_FILE_SUFFIX: &str = ".data";
const MD5_FILE_SUFFIX: &str = ".md5";
const TMP_FILE_SUFFIX: &str = ".tmp";
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct CacheWriteGuard {
    cleanup_paths: Vec<PathBuf>,
    committed: bool,
}

impl CacheWriteGuard {
    fn new(paths: Vec<PathBuf>) -> Self {
        Self {
            cleanup_paths: paths,
            committed: false,
        }
    }

    fn track(&mut self, path: PathBuf) {
        if !self.cleanup_paths.contains(&path) {
            self.cleanup_paths.push(path);
        }
    }

    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for CacheWriteGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for path in &self.cleanup_paths {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// 保存流时返回的句柄，用于完成或放弃缓存
pub struct SaveHandle {
    state: Arc<Mutex<WriterState>>,
}

/// 分片保存句柄，支持逐块写入数据
pub struct ChunkedSaveHandle {
    state: Arc<Mutex<WriterState>>,
}

impl ChunkedSaveHandle {
    /// 写入单个数据块
    pub async fn write_chunk(&self, data: &[u8]) -> Result<(), CacheError> {
        let mut guard = self.state.lock().await;
        if guard.finalized {
            return Err(CacheError::AlreadyFinalized);
        }
        if let Some(ref mut file) = guard.file {
            file.write_all(data).await?;
        } else {
            return Err(CacheError::FileOrContextMissing);
        }
        Ok(())
    }

    /// 完成缓存：同步文件、重命名、写入 MD5
    pub async fn finalize(self, md5: &str) -> Result<(), CacheError> {
        let mut guard = self.state.lock().await;
        if guard.finalized {
            return Err(CacheError::AlreadyFinalized);
        }
        guard.finalized = true;

        if let Some(file) = guard.file.take() {
            file.sync_all().await?;
            tokio::fs::rename(&guard.tmp_path, &guard.data_path).await?;
            tokio::fs::write(&guard.md5_path, md5).await?;
            Ok(())
        } else {
            Err(CacheError::FileOrContextMissing)
        }
    }

    /// 放弃缓存：直接删除临时文件
    pub async fn abort(self) {
        let mut guard = self.state.lock().await;
        if !guard.finalized {
            let _ = tokio::fs::remove_file(&guard.tmp_path).await;
            guard.finalized = true;
        }
    }
}

impl SaveHandle {
    /// 完成缓存：同步文件、重命名、写入 MD5
    /// 如果流过程中发生过错误，则会删除临时文件并返回错误
    pub async fn finalize(self, md5: &str) -> Result<(), CacheError> {
        let mut guard = self.state.lock().await;
        if guard.finalized {
            return Err(CacheError::AlreadyFinalized);
        }
        guard.finalized = true;

        if guard.error_occurred {
            let _ = tokio::fs::remove_file(&guard.tmp_path).await;
            return Err(CacheError::StreamError);
        }

        if let Some(file) = guard.file.take() {
            file.sync_all().await?;
            tokio::fs::rename(&guard.tmp_path, &guard.data_path).await?;

            tokio::fs::write(&guard.md5_path, md5).await?;

            Ok(())
        } else {
            Err(CacheError::FileOrContextMissing)
        }
    }

    /// 放弃缓存：直接删除临时文件
    pub async fn abort(self) {
        let mut guard = self.state.lock().await;
        if !guard.finalized {
            let _ = tokio::fs::remove_file(&guard.tmp_path).await;
            guard.finalized = true;
        }
    }
}

struct WriterState {
    file: Option<tokio::fs::File>,
    tmp_path: PathBuf,
    data_path: PathBuf,
    md5_path: PathBuf,
    error_occurred: bool,
    finalized: bool,
}

#[derive(Debug, Clone)]
pub struct LocalFileCache {
    cache_dir: Arc<PathBuf>,
}

impl LocalFileCache {
    pub fn new(exists_dir: PathBuf) -> Self {
        Self {
            cache_dir: Arc::new(exists_dir),
        }
    }

    fn get_path(&self, key: &str) -> (PathBuf, PathBuf) {
        let data_path = self.cache_dir.join(format!("{}{}", key, DATA_FILE_SUFFIX));
        let md5_path = self.cache_dir.join(format!("{}{}", key, MD5_FILE_SUFFIX));
        (data_path, md5_path)
    }

    fn unique_temp_path(path: &Path) -> PathBuf {
        let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        path.with_extension(format!(
            "{extension}{TMP_FILE_SUFFIX}.{}-{sequence}",
            std::process::id()
        ))
    }

    /// 确保存储文件的父目录存在
    async fn ensure_parent_dir(path: &Path) -> Result<(), CacheError> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        Ok(())
    }

    /// 获取指定 key 的数据文件大小和 MD5 值
    pub async fn get(&self, key: &str) -> Result<Option<String>, CacheError> {
        let (data_path, md5_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let md5_exists = tokio::fs::try_exists(&md5_path).await.unwrap_or(false);
        if data_exists && md5_exists {
            let md5 = tokio::fs::read_to_string(&md5_path)
                .await?
                .trim()
                .to_string();

            Ok(Some(md5))
        } else {
            Ok(None)
        }
    }

    /// 根据key直接返回完整的数据流
    pub async fn get_stream(
        &self,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<ByteStream, CacheError> {
        let (data_path, md5_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let md5_exists = tokio::fs::try_exists(&md5_path).await.unwrap_or(false);
        if data_exists && md5_exists {
            let mut tokio_file = tokio::fs::File::open(&data_path).await?;
            if let Some((start, end)) = range {
                // 将文件指针移动到 start 的位置
                tokio_file.seek(SeekFrom::Start(start)).await?;

                // 计算需要读取的字节数
                let limit = end.saturating_sub(start).saturating_add(1);

                // 限制读取长度并转换为流
                let limited_reader = tokio_file.take(limit);
                let stream = ReaderStream::new(limited_reader);

                Ok(Box::pin(stream))
            } else {
                let stream = ReaderStream::new(tokio_file);
                Ok(Box::pin(stream))
            }
        } else {
            Err(CacheError::NotFound)
        }
    }

    /// 删除指定 key 的缓存文件
    pub async fn delete(&self, key: &str) {
        let (data_path, md5_path) = self.get_path(key);
        let _ = tokio::fs::remove_file(&data_path).await;
        let _ = tokio::fs::remove_file(&md5_path).await;
    }

    /// 直接保存数据文件并计算
    pub async fn save_bytes(&self, key: &str, data: &[u8]) -> Result<(), CacheError> {
        let (data_path, md5_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let tmp_path = data_path.with_extension(format!("{}{}", DATA_FILE_SUFFIX, TMP_FILE_SUFFIX));

        // 异步写入临时文件
        tokio::fs::write(&tmp_path, data).await?;

        // 原子重命名
        tokio::fs::rename(&tmp_path, &data_path).await?;

        let md5 = format!("{:X}", md5::compute(data));
        tokio::fs::write(&md5_path, &md5).await?;

        Ok(())
    }

    /// 分片保存：返回分片句柄，支持逐块写入
    pub async fn begin_chunked_save(&self, key: &str) -> Result<ChunkedSaveHandle, CacheError> {
        let (data_path, md5_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let tmp_path = data_path.with_extension(format!("{}{}", DATA_FILE_SUFFIX, TMP_FILE_SUFFIX));
        let file = tokio::fs::File::create(&tmp_path).await?;

        let state = Arc::new(Mutex::new(WriterState {
            file: Some(file),
            tmp_path,
            data_path,
            md5_path,
            error_occurred: false,
            finalized: false,
        }));

        Ok(ChunkedSaveHandle { state })
    }

    /// 流式保存：返回包装后的流和完成句柄
    /// 消费流时数据会同时写入临时文件
    /// 流结束后必须调用 `handle.finalize()` 或 `handle.abort()`
    pub async fn save(
        &self,
        key: &str,
        stream: ByteStream,
    ) -> Result<(ByteStream, SaveHandle), CacheError> {
        let (data_path, md5_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let tmp_path = data_path.with_extension(format!("{}{}", DATA_FILE_SUFFIX, TMP_FILE_SUFFIX));
        let file = tokio::fs::File::create(&tmp_path).await?;

        let state = Arc::new(Mutex::new(WriterState {
            file: Some(file),
            tmp_path,
            data_path,
            md5_path,
            error_occurred: false,
            finalized: false,
        }));

        // 包装原始流
        let state_clone = state.clone();
        let wrapped_stream: ByteStream = Box::pin(stream.then(move |chunk_result| {
            let state = state_clone.clone();
            async move {
                match chunk_result {
                    Ok(chunk) => {
                        let mut file = {
                            let mut guard = state.lock().await;
                            if guard.finalized {
                                return Err(std::io::Error::other("Stream finalized"));
                            }
                            guard
                                .file
                                .take()
                                .ok_or_else(|| std::io::Error::other("File closed"))?
                        };

                        let write_res = file.write_all(&chunk).await;

                        let mut guard = state.lock().await;
                        if let Err(e) = write_res {
                            guard.error_occurred = true;
                            guard.file = Some(file);
                            return Err(e);
                        }
                        guard.file = Some(file);

                        Ok(chunk)
                    }
                    Err(e) => {
                        let mut guard = state.lock().await;
                        guard.error_occurred = true;
                        Err(e)
                    }
                }
            }
        }));

        Ok((wrapped_stream, SaveHandle { state }))
    }

    /// 流式保存数据，并在完整写入后使用远端 ETag 固化缓存。
    ///
    /// 数据下载期间保留原缓存；流失败或任务取消时会清理临时文件。
    pub async fn save_stream_with_etag(
        &self,
        key: &str,
        etag: &str,
        mut stream: ByteStream,
    ) -> Result<(), CacheError> {
        if etag.trim().is_empty() {
            return Err(CacheError::InvalidEtag);
        }

        let (data_path, md5_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let data_tmp_path = Self::unique_temp_path(&data_path);
        let etag_tmp_path = Self::unique_temp_path(&md5_path);
        let mut cleanup = CacheWriteGuard::new(vec![data_tmp_path.clone(), etag_tmp_path.clone()]);
        let mut data_file = tokio::fs::File::create(&data_tmp_path).await?;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| CacheError::StreamError)?;
            data_file.write_all(&chunk).await?;
        }
        data_file.sync_all().await?;
        drop(data_file);

        let mut etag_file = tokio::fs::File::create(&etag_tmp_path).await?;
        etag_file.write_all(etag.trim().as_bytes()).await?;
        etag_file.sync_all().await?;
        drop(etag_file);

        // Windows 不允许 rename 覆盖现有文件。先移除 ETag，使替换过程中的缓存不可见。
        cleanup.track(data_path.clone());
        cleanup.track(md5_path.clone());
        let _ = tokio::fs::remove_file(&md5_path).await;
        let _ = tokio::fs::remove_file(&data_path).await;

        tokio::fs::rename(&data_tmp_path, &data_path).await?;
        tokio::fs::rename(&etag_tmp_path, &md5_path).await?;

        cleanup.commit();
        Ok(())
    }

    /// 根据key直接返回完整的数据
    pub async fn get_data(&self, key: &str) -> Result<Vec<u8>, CacheError> {
        let (data_path, md5_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let md5_exists = tokio::fs::try_exists(&md5_path).await.unwrap_or(false);
        if data_exists && md5_exists {
            Ok(tokio::fs::read(&data_path).await?)
        } else {
            Err(CacheError::NotFound)
        }
    }

    /// 删除所有缓存
    pub async fn delete_all(&self) -> Result<(), CacheError> {
        let mut read_dir = tokio::fs::read_dir(self.cache_dir.as_path()).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                tokio::fs::remove_dir_all(entry.path()).await?;
            } else {
                tokio::fs::remove_file(entry.path()).await?;
            }
        }
        Ok(())
    }

    /// 获取所有缓存文件信息(Key, Md5)
    pub async fn get_all(&self) -> Result<Vec<(String, String)>, CacheError> {
        let mut results = Vec::new();
        let mut stack = vec![self.cache_dir.as_ref().to_path_buf()];

        while let Some(dir) = stack.pop() {
            let mut read_dir = tokio::fs::read_dir(&dir).await?;

            while let Some(entry) = read_dir.next_entry().await? {
                let file_type = entry.file_type().await?;
                #[cfg(debug_assertions)]
                println!("{:?}, filetype: {}", entry.path(), file_type.is_file());
                if file_type.is_dir() {
                    stack.push(entry.path());
                    continue;
                }
                let path = entry.path();
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or(CacheError::InvalidFilename)?;
                if !file_name.ends_with(DATA_FILE_SUFFIX) {
                    continue;
                }
                // 获取相对路径
                let relative = path
                    .strip_prefix(self.cache_dir.as_path())
                    .map_err(|e| CacheError::PathError(e.to_string()))?;
                let key_with_sep = relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if let Some(key) = key_with_sep.strip_suffix(DATA_FILE_SUFFIX) {
                    let md5_path = self.cache_dir.join(format!("{}{}", key, MD5_FILE_SUFFIX));
                    if md5_path.exists() {
                        if let Ok(md5) = tokio::fs::read_to_string(&md5_path).await {
                            results.push((key.to_string(), md5.trim().to_string()));
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}
