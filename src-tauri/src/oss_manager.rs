use aliyun_oss_client::{Bucket, Client, EndPoint, Key, Object, Secret};
use std::sync::{Arc, Mutex};
use aliyun_oss_client::types::ObjectQuery;

/// 错误类型封装
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum OssError {
    #[error("OSS API 错误: {0}")]
    ApiError(String),
    #[error("无效的 Endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("客户端未初始化")]
    Uninitialized,
    #[error("内部锁失败")]
    LockError,
}

// 帮助宏，将 aliyun-oss-client 的错误转换为 OssError::ApiError
macro_rules! oss_api_err {
    ($e:expr) => {
        $e.map_err(|e| OssError::ApiError(e.to_string()))
    };
}


/// 核心状态管理结构体
/// 使用 Mutex 允许在多个 Tauri 命令线程中安全地共享和修改客户端实例。
#[derive(Default)]
pub struct OssClientManager(pub Mutex<Option<Arc<Client>>>);

#[allow(dead_code)]
impl OssClientManager {
    /// 辅助方法：获取 Arc<Client> 的线程安全克隆
    fn get_client(&self) -> Result<Arc<Client>, OssError> {
        let guard = self.0.lock().map_err(|_| OssError::LockError)?;
        guard.as_ref().cloned().ok_or(OssError::Uninitialized)
    }

    /// 初始化 OSS 客户端并设置到状态中
    /// 包含 AK/SK 和连接的验证逻辑
    /// https://oss.console.aliyun.com/overview
    pub async fn initialize(
        &self,
        access_key_id: &str,
        access_key_secret: &str,
        endpoint_url: &str,
        bucket_name: &str,
    ) -> Result<(), OssError> {
        let ep = EndPoint::new(endpoint_url)
            .map_err(|e| OssError::InvalidEndpoint(e.to_string()))?;

        let key = Key::new(access_key_id);
        let secret = Secret::new(access_key_secret);
        let mut client = Client::new(key, secret);

        // 1. 验证连接
        oss_api_err!(client.get_buckets(&ep).await)?;

        // 2. 设置 bucket
        let bucket = Bucket::new(bucket_name.to_string(), ep.clone());
        client.set_bucket(bucket);

        // 3. 将客户端存入 Mutex
        let mut client_guard = self.0.lock().map_err(|_| OssError::LockError)?;
        *client_guard = Some(Arc::new(client));

        Ok(())
    }

    /// 上传数据到 OSS
    pub async fn upload_object(
        &self,
        object_key: &str,
        data: Vec<u8>,
    ) -> Result<(), OssError> {
        let client = self.get_client()?;

        oss_api_err!(
            Object::new(object_key)
                .upload(data, &client)
                .await
        )?;

        Ok(())
    }

    /// 从 OSS 下载数据
    pub async fn download_object(
        &self,
        object_key: &str,
    ) -> Result<Vec<u8>, OssError> {
        let client = self.get_client()?;

        let data = oss_api_err!(
            Object::new(object_key)
                .download(&client)
                .await
        )?;

        Ok(data)
    }

    /// 删除 OSS 对象
    pub async fn delete_object(
        &self,
        object_key: &str,
    ) -> Result<(), OssError> {
        let client = self.get_client()?;

        oss_api_err!(
            Object::new(object_key)
                .delete(&client)
                .await
        )?;

        Ok(())
    }

    /// 列出指定前缀下的所有对象路径 https://help.aliyun.com/zh/oss/developer-reference/listobjectsv2
    pub async fn list_objects(
        &self,
        prefix: &str,
    ) -> Result<Vec<String>, OssError> {
        let client = self.get_client()?;
        let bucket = client.bucket().ok_or(OssError::Uninitialized)?;

        let mut all_objects = Vec::new();
        let condition = {
            let mut map = ObjectQuery::new();
            map.insert(ObjectQuery::MAX_KEYS, "1000");
            map.insert(ObjectQuery::PREFIX, prefix);
            map
        };

        let mut current_objects = oss_api_err!(
            bucket.get_objects(&condition, &client).await
        )?;

        loop {
            // 将当前批次的对象路径添加到总列表中
            for object in current_objects.get_vec() {
                all_objects.push(object.get_path().to_string());
            }

            // 检查是否有下一页
            if current_objects.next_token().is_some() {
                // 如果有下一页，获取下一页数据
                current_objects = oss_api_err!(
                    current_objects.next_list(&condition, &client).await
                )?;
            } else {
                // 没有下一页，跳出循环
                break;
            }
        }

        Ok(all_objects)
    }
}