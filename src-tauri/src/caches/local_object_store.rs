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

// 保留旧目录名，避免纯代码重命名导致现有本地数据不可见。
pub const LOCAL_OBJECT_STORE_DIRECTORY: &str = "lfc";

const DATA_FILE_SUFFIX: &str = ".data";
// `.md5` 是旧版本沿用的磁盘格式；文件内容现在表示对象 ETag，不保证一定是 MD5。
const ETAG_FILE_SUFFIX: &str = ".md5";
const TMP_FILE_SUFFIX: &str = ".tmp";
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct ObjectWriteGuard {
    cleanup_paths: Vec<PathBuf>,
    committed: bool,
}

impl ObjectWriteGuard {
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

impl Drop for ObjectWriteGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for path in &self.cleanup_paths {
            let _ = std::fs::remove_file(path);
        }
    }
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

    /// 完成对象写入：同步文件、重命名、写入 ETag 标记
    pub async fn finalize(self, etag: &str) -> Result<(), CacheError> {
        let mut guard = self.state.lock().await;
        if guard.finalized {
            return Err(CacheError::AlreadyFinalized);
        }
        guard.finalized = true;

        if etag.trim().is_empty() {
            guard.file.take();
            let _ = tokio::fs::remove_file(&guard.tmp_path).await;
            return Err(CacheError::InvalidEtag);
        }

        if let Some(file) = guard.file.take() {
            if let Err(error) = file.sync_all().await {
                let _ = tokio::fs::remove_file(&guard.tmp_path).await;
                return Err(error.into());
            }
            if let Err(error) = tokio::fs::rename(&guard.tmp_path, &guard.data_path).await {
                let _ = tokio::fs::remove_file(&guard.tmp_path).await;
                return Err(error.into());
            }
            if let Err(error) = tokio::fs::write(&guard.etag_path, etag).await {
                let _ = tokio::fs::remove_file(&guard.data_path).await;
                let _ = tokio::fs::remove_file(&guard.etag_path).await;
                return Err(error.into());
            }
            Ok(())
        } else {
            Err(CacheError::FileOrContextMissing)
        }
    }

    /// 放弃对象写入：直接删除临时文件
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
    etag_path: PathBuf,
    finalized: bool,
}

#[derive(Debug, Clone)]
pub struct LocalObjectStore {
    store_dir: Arc<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalObjectEntry {
    pub key: String,
    pub etag: String,
    pub size: u64,
}

impl LocalObjectStore {
    pub fn new(exists_dir: PathBuf) -> Self {
        Self {
            store_dir: Arc::new(exists_dir),
        }
    }

    fn get_path(&self, key: &str) -> (PathBuf, PathBuf) {
        let data_path = self.store_dir.join(format!("{}{}", key, DATA_FILE_SUFFIX));
        let etag_path = self.store_dir.join(format!("{}{}", key, ETAG_FILE_SUFFIX));
        (data_path, etag_path)
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

    /// 获取指定 key 的 ETag；对象不存在或不完整时返回 `None`
    pub async fn get(&self, key: &str) -> Result<Option<String>, CacheError> {
        let (data_path, etag_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let etag_exists = tokio::fs::try_exists(&etag_path).await.unwrap_or(false);
        if data_exists && etag_exists {
            let etag = tokio::fs::read_to_string(&etag_path)
                .await?
                .trim()
                .to_string();

            Ok(Some(etag))
        } else {
            Ok(None)
        }
    }

    /// 获取有效对象的数据文件大小。
    ///
    /// 数据文件和 ETag 标记必须同时存在；不完整对象按不存在处理。
    pub async fn get_size(&self, key: &str) -> Result<Option<u64>, CacheError> {
        let (data_path, etag_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let etag_exists = tokio::fs::try_exists(&etag_path).await.unwrap_or(false);
        if data_exists && etag_exists {
            Ok(Some(tokio::fs::metadata(data_path).await?.len()))
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
        let (data_path, etag_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let etag_exists = tokio::fs::try_exists(&etag_path).await.unwrap_or(false);
        if data_exists && etag_exists {
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

    /// 删除指定 key 的本地对象
    pub async fn delete(&self, key: &str) -> Result<(), CacheError> {
        let (data_path, etag_path) = self.get_path(key);
        // 先删除真正的数据，再删除 ETag 标记。如果数据文件因占用等原因删除失败，
        // 标记仍然存在，后续重试仍能枚举到该对象。
        for path in [&data_path, &etag_path] {
            match tokio::fs::remove_file(path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    /// 直接保存数据文件并计算
    pub async fn save_bytes(&self, key: &str, data: &[u8]) -> Result<(), CacheError> {
        let (data_path, etag_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let tmp_path = data_path.with_extension(format!("{}{}", DATA_FILE_SUFFIX, TMP_FILE_SUFFIX));

        // 异步写入临时文件
        tokio::fs::write(&tmp_path, data).await?;

        // 原子重命名
        tokio::fs::rename(&tmp_path, &data_path).await?;

        let etag = format!("{:X}", md5::compute(data));
        tokio::fs::write(&etag_path, &etag).await?;

        Ok(())
    }

    /// 分片保存：返回分片句柄，支持逐块写入
    pub async fn begin_chunked_save(&self, key: &str) -> Result<ChunkedSaveHandle, CacheError> {
        let (data_path, etag_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let tmp_path = data_path.with_extension(format!("{}{}", DATA_FILE_SUFFIX, TMP_FILE_SUFFIX));
        let file = tokio::fs::File::create(&tmp_path).await?;

        let state = Arc::new(Mutex::new(WriterState {
            file: Some(file),
            tmp_path,
            data_path,
            etag_path,
            finalized: false,
        }));

        Ok(ChunkedSaveHandle { state })
    }

    /// 流式保存数据，并在完整写入后使用给定 ETag 提交对象。
    ///
    /// 写入期间保留原对象；流失败或任务取消时会清理临时文件。
    pub async fn save_stream_with_etag(
        &self,
        key: &str,
        etag: &str,
        mut stream: ByteStream,
    ) -> Result<(), CacheError> {
        if etag.trim().is_empty() {
            return Err(CacheError::InvalidEtag);
        }

        let (data_path, etag_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let data_tmp_path = Self::unique_temp_path(&data_path);
        let etag_tmp_path = Self::unique_temp_path(&etag_path);
        let mut cleanup = ObjectWriteGuard::new(vec![data_tmp_path.clone(), etag_tmp_path.clone()]);
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

        // Windows 不允许 rename 覆盖现有文件。先移除 ETag，使替换过程中的对象不可见。
        cleanup.track(data_path.clone());
        cleanup.track(etag_path.clone());
        let _ = tokio::fs::remove_file(&etag_path).await;
        let _ = tokio::fs::remove_file(&data_path).await;

        tokio::fs::rename(&data_tmp_path, &data_path).await?;
        tokio::fs::rename(&etag_tmp_path, &etag_path).await?;

        cleanup.commit();
        Ok(())
    }

    /// 只更新现有对象的 ETag 标记，不重复写入数据文件。
    pub async fn set_etag(&self, key: &str, etag: &str) -> Result<(), CacheError> {
        if etag.trim().is_empty() {
            return Err(CacheError::InvalidEtag);
        }
        let (data_path, etag_path) = self.get_path(key);
        if !tokio::fs::try_exists(&data_path).await.unwrap_or(false) {
            return Err(CacheError::NotFound);
        }

        let etag_tmp_path = Self::unique_temp_path(&etag_path);
        let mut cleanup = ObjectWriteGuard::new(vec![etag_tmp_path.clone()]);
        let mut etag_file = tokio::fs::File::create(&etag_tmp_path).await?;
        etag_file.write_all(etag.trim().as_bytes()).await?;
        etag_file.sync_all().await?;
        drop(etag_file);

        cleanup.track(etag_path.clone());
        let _ = tokio::fs::remove_file(&etag_path).await;
        tokio::fs::rename(&etag_tmp_path, &etag_path).await?;
        cleanup.commit();
        Ok(())
    }

    /// 根据key直接返回完整的数据
    pub async fn get_data(&self, key: &str) -> Result<Vec<u8>, CacheError> {
        let (data_path, etag_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let etag_exists = tokio::fs::try_exists(&etag_path).await.unwrap_or(false);
        if data_exists && etag_exists {
            Ok(tokio::fs::read(&data_path).await?)
        } else {
            Err(CacheError::NotFound)
        }
    }

    /// 删除所有本地对象
    pub async fn delete_all(&self) -> Result<(), CacheError> {
        let mut read_dir = tokio::fs::read_dir(self.store_dir.as_path()).await?;
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

    /// 获取所有有效本地对象的信息。
    pub async fn get_all_entries(&self) -> Result<Vec<LocalObjectEntry>, CacheError> {
        let mut results = Vec::new();
        let mut stack = vec![self.store_dir.as_ref().to_path_buf()];

        while let Some(dir) = stack.pop() {
            let mut read_dir = match tokio::fs::read_dir(&dir).await {
                Ok(read_dir) => read_dir,
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && dir.as_path() == self.store_dir.as_path() =>
                {
                    // 尚未产生任何本地对象，或存储目录被系统清理时，按空存储处理。
                    return Ok(results);
                }
                Err(error) => return Err(error.into()),
            };

            while let Some(entry) = read_dir.next_entry().await? {
                let file_type = entry.file_type().await?;
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
                    .strip_prefix(self.store_dir.as_path())
                    .map_err(|e| CacheError::PathError(e.to_string()))?;
                let key_with_sep = relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if let Some(key) = key_with_sep.strip_suffix(DATA_FILE_SUFFIX) {
                    let etag_path = self.store_dir.join(format!("{}{}", key, ETAG_FILE_SUFFIX));
                    if etag_path.exists() {
                        if let Ok(etag) = tokio::fs::read_to_string(&etag_path).await {
                            let size = tokio::fs::metadata(&path).await?.len();
                            results.push(LocalObjectEntry {
                                key: key.to_string(),
                                etag: etag.trim().to_string(),
                                size,
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// 获取所有本地对象的 `(Key, ETag)`。
    pub async fn get_all(&self) -> Result<Vec<(String, String)>, CacheError> {
        Ok(self
            .get_all_entries()
            .await?
            .into_iter()
            .map(|entry| (entry.key, entry.etag))
            .collect())
    }
}
