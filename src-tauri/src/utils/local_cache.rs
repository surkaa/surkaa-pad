use crate::object::ByteStream;
use futures_util::{StreamExt};
use std::io::Write;
use std::path::PathBuf;
use tokio_util::io::ReaderStream;

const DATA_FILE_SUFFIX: &str = ".data";
const MD5_FILE_SUFFIX: &str = ".md5";

pub struct LocalCache {
    cache_dir: PathBuf,
}

impl LocalCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: dir.into(),
        }
    }

    fn get_path(&self, key: &str) -> (PathBuf, PathBuf) {
        let data_path = self.cache_dir.join(format!("{}{}", key, DATA_FILE_SUFFIX));
        let md5_path = self.cache_dir.join(format!("{}{}", key, MD5_FILE_SUFFIX));
        (data_path, md5_path)
    }

    /// 获取指定 key 的数据文件大小和 MD5 值
    pub fn get(&self, key: &str) -> Result<(u64, String), String> {
        let (data_path, md5_path) = self.get_path(key);
        if data_path.exists() && md5_path.exists() {
            // 读取数据文件的大小
            let size = std::fs::metadata(&data_path)
                .map_err(|e| format!("无法获取数据文件元信息: {}", e))?
                .len();
            // 读取 MD5 文件的内容
            let md5 = std::fs::read_to_string(&md5_path)
                .map_err(|e| format!("无法读取 MD5 文件: {}", e))?
                .trim()
                .to_string();
            Ok((size, md5))
        } else {
            Err("缓存不存在".to_string())
        }
    }

    /// 删除指定 key 的缓存文件
    pub fn delete(&self, key: &str) -> Result<(), String> {
        let (data_path, md5_path) = self.get_path(key);
        if data_path.exists() {
            std::fs::remove_file(&data_path).map_err(|e| format!("无法删除数据文件: {}", e))?;
        }
        if md5_path.exists() {
            std::fs::remove_file(&md5_path).map_err(|e| format!("无法删除 MD5 文件: {}", e))?;
        }
        Ok(())
    }

    /// 直接保存数据文件并计算和返回 MD5 值
    pub fn save_bytes(&self, key: &str, data: &[u8]) -> Result<String, String> {
        self.delete(key)?;
        let (data_path, md5_path) = self.get_path(key);
        // 保存数据文件
        std::fs::write(&data_path, data).map_err(|e| format!("无法保存数据文件: {}", e))?;
        // 计算 MD5 值
        let md5 = format!("{:x}", md5::compute(data));
        // 保存 MD5 文件
        std::fs::write(&md5_path, &md5).map_err(|e| format!("无法保存 MD5 文件: {}", e))?;
        Ok(md5)
    }

    /// 流式保存数据文件并计算和返回 MD5 值
    pub async fn save(&self, key: &str, mut stream: ByteStream) -> Result<String, String> {
        self.delete(key)?;
        let (data_path, md5_path) = self.get_path(key);
        // 创建数据文件
        let mut file =
            std::fs::File::create(&data_path).map_err(|e| format!("无法创建数据文件: {}", e))?;
        // 创建 MD5 计算器
        let mut context = md5::Context::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("读取数据流失败: {}", e))?;
            // 写入数据文件
            file.write_all(&chunk)
                .map_err(|e| format!("无法写入数据文件: {}", e))?;
            // 更新 MD5 计算器
            context.consume(&chunk);
        }
        // 计算 MD5 值
        let md5 = format!("{:x}", context.finalize());
        // 保存 MD5 文件
        std::fs::write(&md5_path, &md5).map_err(|e| format!("无法保存 MD5 文件: {}", e))?;
        Ok(md5)
    }

    /// 根据key直接返回完整的数据
    pub fn get_data(&self, key: &str) -> Result<Vec<u8>, String> {
        let (data_path, md5_path) = self.get_path(key);
        if data_path.exists() && md5_path.exists() {
            std::fs::read(&data_path).map_err(|e| format!("无法读取数据文件: {}", e))
        } else {
            Err("缓存不存在".to_string())
        }
    }

    /// 根据key直接返回完整的数据流
    pub async fn get_stream(&self, key: &str) -> Result<ByteStream, String> {
        let (data_path, md5_path) = self.get_path(key);
        if data_path.exists() && md5_path.exists() {
            let tokio_file = tokio::fs::File::open(&data_path)
                .await
                .map_err(|e| format!("无法打开数据文件: {}", e))?;
            let stream = ReaderStream::new(tokio_file);
            Ok(Box::pin(stream))
        } else {
            Err("缓存不存在".to_string())
        }
    }
}
