use crate::stream::ByteStream;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;

pub const LOCAL_FILE_CACHE_FILENAME: &str = "lfc";

const DATA_FILE_SUFFIX: &str = ".data";
const MD5_FILE_SUFFIX: &str = ".md5";
const TMP_FILE_SUFFIX: &str = ".tmp";

/// 保存流时返回的句柄，用于完成或放弃缓存
pub struct SaveHandle {
    state: Arc<Mutex<WriterState>>,
}

impl SaveHandle {
    /// 完成缓存：同步文件、重命名、写入 MD5
    /// 如果流过程中发生过错误，则会删除临时文件并返回错误
    pub async fn finalize(self) -> Result<(), String> {
        let mut guard = self.state.lock().await;
        if guard.finalized {
            return Err("Already finalized".to_string());
        }
        guard.finalized = true;

        if guard.error_occurred {
            let _ = tokio::fs::remove_file(&guard.tmp_path).await;
            return Err("Error occurred during streaming, cache not saved".to_string());
        }

        if let (Some(file), Some(context)) = (guard.file.take(), guard.context.take()) {
            file.sync_all().await.map_err(|e| e.to_string())?;
            tokio::fs::rename(&guard.tmp_path, &guard.data_path)
                .await
                .map_err(|e| e.to_string())?;

            // 计算 MD5
            let md5 = format!("{:X}", context.finalize());
            tokio::fs::write(&guard.md5_path, &md5)
                .await
                .map_err(|e| e.to_string())?;

            Ok(())
        } else {
            Err("file or context missing".to_string())
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
    context: Option<md5::Context>,
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

    #[cfg(test)]
    pub fn new_test() -> Self {
        let path = tempfile::tempdir()
            .expect("无法创建临时目录")
            .path()
            .to_path_buf();
        Self {
            cache_dir: Arc::new(path),
        }
    }

    fn get_path(&self, key: &str) -> (PathBuf, PathBuf) {
        let path = self.cache_dir.join(key);
        let path_full = path.extension().unwrap_or_default().to_string_lossy();
        let data_path = path.with_extension(format!("{}{}", &path_full, DATA_FILE_SUFFIX));
        let md5_path = path.with_extension(format!("{}{}", &path_full, MD5_FILE_SUFFIX));
        (data_path, md5_path)
    }

    /// 确保存储文件的父目录存在
    async fn ensure_parent_dir(path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("无法创建父目录: {}", e))?;
            }
        }
        Ok(())
    }

    /// 获取指定 key 的数据文件大小和 MD5 值
    pub async fn get(&self, key: &str) -> Result<Option<(u64, String)>, String> {
        let (data_path, md5_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let md5_exists = tokio::fs::try_exists(&md5_path).await.unwrap_or(false);
        if data_exists && md5_exists {
            let size = tokio::fs::metadata(&data_path)
                .await
                .map_err(|e| format!("无法获取数据文件元信息: {}", e))?
                .len();

            let md5 = tokio::fs::read_to_string(&md5_path)
                .await
                .map_err(|e| format!("无法读取 MD5 文件: {}", e))?
                .trim()
                .to_string();

            Ok(Some((size, md5)))
        } else {
            Ok(None)
        }
    }

    /// 根据key直接返回完整的数据流
    pub async fn get_stream(&self, key: &str) -> Result<ByteStream, String> {
        let (data_path, md5_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let md5_exists = tokio::fs::try_exists(&md5_path).await.unwrap_or(false);
        if data_exists && md5_exists {
            let tokio_file = tokio::fs::File::open(&data_path)
                .await
                .map_err(|e| format!("无法打开数据文件: {}", e))?;
            let stream = ReaderStream::new(tokio_file);
            Ok(Box::pin(stream))
        } else {
            Err("缓存不存在".to_string())
        }
    }

    /// 删除指定 key 的缓存文件
    pub async fn delete(&self, key: &str) {
        let (data_path, md5_path) = self.get_path(key);
        let _ = tokio::fs::remove_file(&data_path).await;
        let _ = tokio::fs::remove_file(&md5_path).await;
    }

    /// 直接保存数据文件并计算
    pub async fn save_bytes(&self, key: &str, data: &[u8]) -> Result<(), String> {
        let (data_path, md5_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let tmp_path = data_path.with_extension(format!("{}{}", DATA_FILE_SUFFIX, TMP_FILE_SUFFIX));

        // 异步写入临时文件
        tokio::fs::write(&tmp_path, data)
            .await
            .map_err(|e| format!("无法保存数据文件: {}", e))?;

        // 原子重命名
        tokio::fs::rename(&tmp_path, &data_path)
            .await
            .map_err(|e| format!("数据文件重命名失败: {}", e))?;

        let md5 = format!("{:X}", md5::compute(data));
        tokio::fs::write(&md5_path, &md5)
            .await
            .map_err(|e| format!("无法保存 MD5 文件: {}", e))?;

        Ok(())
    }

    /// 流式保存：返回包装后的流和完成句柄
    /// 消费流时数据会同时写入临时文件并计算 MD5
    /// 流结束后必须调用 `handle.finalize()` 或 `handle.abort()`
    pub async fn save(
        &self,
        key: &str,
        stream: ByteStream,
    ) -> Result<(ByteStream, SaveHandle), String> {
        let (data_path, md5_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let tmp_path = data_path.with_extension(format!("{}{}", DATA_FILE_SUFFIX, TMP_FILE_SUFFIX));
        let file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| format!("无法创建数据文件: {}", e))?;

        let state = Arc::new(Mutex::new(WriterState {
            file: Some(file),
            context: Some(md5::Context::new()),
            tmp_path,
            data_path,
            md5_path,
            error_occurred: false,
            finalized: false,
        }));

        // 包装原始流
        let wrapped_stream = {
            let state = state.clone();
            stream
                .then(move |chunk_result| {
                    let state = state.clone();
                    async move {
                        let mut guard = state.lock().await;
                        if guard.finalized {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "Stream already finalized",
                            ));
                        }
                        match chunk_result {
                            Ok(chunk) => {
                                if let Some(file) = guard.file.as_mut() {
                                    if let Err(e) = file.write_all(&chunk).await {
                                        guard.error_occurred = true;
                                        return Err(e);
                                    }
                                    // 使用 context 的可变引用更新 MD5
                                    if let Some(context) = guard.context.as_mut() {
                                        context.consume(&chunk);
                                    }
                                    Ok(chunk)
                                } else {
                                    guard.error_occurred = true;
                                    Err(std::io::Error::new(
                                        std::io::ErrorKind::Other,
                                        "File closed",
                                    ))
                                }
                            }
                            Err(e) => {
                                guard.error_occurred = true;
                                Err(e)
                            }
                        }
                    }
                })
                .boxed()
        };

        Ok((Box::pin(wrapped_stream), SaveHandle { state }))
    }

    /// 根据key直接返回完整的数据
    pub async fn get_data(&self, key: &str) -> Result<Vec<u8>, String> {
        let (data_path, md5_path) = self.get_path(key);
        let data_exists = tokio::fs::try_exists(&data_path).await.unwrap_or(false);
        let md5_exists = tokio::fs::try_exists(&md5_path).await.unwrap_or(false);
        if data_exists && md5_exists {
            tokio::fs::read(&data_path)
                .await
                .map_err(|e| format!("无法读取数据文件: {}", e))
        } else {
            Err("缓存不存在".to_string())
        }
    }
}
