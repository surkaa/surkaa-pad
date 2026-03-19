use crate::stream::ByteStream;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

const DATA_FILE_SUFFIX: &str = ".data";
const MD5_FILE_SUFFIX: &str = ".md5";
const TMP_FILE_SUFFIX: &str = ".tmp";

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
    pub async fn get(&self, key: &str) -> Result<(u64, String), String> {
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

            Ok((size, md5))
        } else {
            Err("缓存不存在".to_string())
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

    /// 直接保存数据文件并计算和返回 MD5 值
    pub async fn save_bytes(&self, key: &str, data: &[u8]) -> Result<String, String> {
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

        let md5 = format!("{:x}", md5::compute(data));
        tokio::fs::write(&md5_path, &md5)
            .await
            .map_err(|e| format!("无法保存 MD5 文件: {}", e))?;

        Ok(md5)
    }

    /// 流式保存数据文件并计算和返回 MD5 值
    pub async fn save(&self, key: &str, mut stream: ByteStream) -> Result<String, String> {
        let (data_path, md5_path) = self.get_path(key);
        Self::ensure_parent_dir(&data_path).await?;

        let tmp_path = data_path.with_extension(format!("{}{}", DATA_FILE_SUFFIX, TMP_FILE_SUFFIX));
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| format!("无法创建数据文件: {}", e))?;

        let mut context = md5::Context::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("读取数据流失败: {}", e))?;
            // 写入数据文件
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("无法写入数据文件: {}", e))?;
            // 更新 MD5 计算器
            context.consume(&chunk);
        }

        // 确保数据落盘
        file.sync_all()
            .await
            .map_err(|e| format!("同步磁盘失败: {}", e))?;

        // 原子重命名
        tokio::fs::rename(&tmp_path, &data_path)
            .await
            .map_err(|e| format!("数据文件重命名失败: {}", e))?;

        let md5 = format!("{:x}", context.finalize());
        tokio::fs::write(&md5_path, &md5)
            .await
            .map_err(|e| format!("无法保存 MD5 文件: {}", e))?;

        Ok(md5)
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
