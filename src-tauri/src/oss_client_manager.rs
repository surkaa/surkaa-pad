use aliyun_oss_client::types::ObjectQuery;
use aliyun_oss_client::{Bucket, Client, EndPoint, Key, Object, Secret};
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct ObjectInfo {
    filename: String,
    size: u64,
    etag: String,
    modified: DateTime<Utc>,
}

/// 核心状态管理结构体
/// 使用 Mutex 允许在多个 Tauri 命令线程中安全地共享和修改客户端实例。
#[derive(Default)]
pub struct OssClientManager(pub Mutex<Option<Arc<Client>>>);

/// 用于管理 OSS 客户端的实现 只用于上传、下载、删除和列出对象列表
#[allow(dead_code)]
impl OssClientManager {
    /// 辅助方法：获取 Arc<Client> 的线程安全克隆
    fn get_client(&self) -> Result<Arc<Client>, String> {
        let guard = self
            .0
            .lock()
            .map_err(|e| format!("Failed to lock OSS client: {}", e))?;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "OSS client is not initialized".to_string())
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
    ) -> Result<(), String> {
        let ep = EndPoint::new(endpoint_url)
            .map_err(|e| format!("Failed to create endpoint URL: {}", e))?;

        let key = Key::new(access_key_id);
        let secret = Secret::new(access_key_secret);
        let mut client = Client::new(key, secret);

        // 1. 验证连接
        client
            .get_buckets(&ep)
            .await
            .map_err(|e| format!("Failed to validate AK/SK: {}", e))?;

        // 2. 设置 bucket
        let bucket = Bucket::new(bucket_name.to_string(), ep.clone());
        client.set_bucket(bucket);

        // 3. 将客户端存入 Mutex
        let mut client_guard = self
            .0
            .lock()
            .map_err(|e| format!("Failed to lock OSS client: {}", e))?;
        *client_guard = Some(Arc::new(client));

        Ok(())
    }

    /// 上传数据到 OSS
    pub async fn upload_object(&self, object_key: &str, data: Vec<u8>) -> Result<(), String> {
        let client = self.get_client()?;

        Object::new(object_key)
            .upload(data, &client)
            .await
            .map_err(|e| format!("Failed to upload object: {}", e))?;

        Ok(())
    }

    /// 从 OSS 下载数据
    pub async fn download_object(&self, object_key: &str) -> Result<Vec<u8>, String> {
        let client = self.get_client()?;

        let data = Object::new(object_key)
            .download(&client)
            .await
            .map_err(|e| format!("Failed to download object: {}", e))?;

        Ok(data)
    }

    /// 删除 OSS 对象
    pub async fn delete_object(&self, object_key: &str) -> Result<(), String> {
        let client = self.get_client()?;

        Object::new(object_key)
            .delete(&client)
            .await
            .map_err(|e| format!("Failed to delete object: {}", e))?;

        Ok(())
    }

    /// 列出指定前缀下的所有对象路径 自动去掉文件夹末尾的斜杠 https://help.aliyun.com/zh/oss/developer-reference/listobjectsv2
    pub async fn list_objects(&self, prefix: &str) -> Result<Vec<ObjectInfo>, String> {
        let client = self.get_client()?;
        let bucket = client
            .bucket()
            .ok_or_else(|| "Bucket is not set in OSS client".to_string())?;

        let mut all_objects = Vec::new();
        let condition = {
            let mut map = ObjectQuery::new();
            map.insert(ObjectQuery::MAX_KEYS, "1000");
            map.insert(ObjectQuery::PREFIX, prefix);
            map
        };

        let mut current_objects = bucket
            .get_objects(&condition, &client)
            .await
            .map_err(|e| format!("Failed to list objects: {}", e))?;

        loop {
            // 将当前批次的对象路径添加到总列表中
            for object in current_objects.get_vec() {
                let info = object
                    .get_info(&client)
                    .await
                    .map_err(|e| format!("Failed to get object: {}", e))?;
                // 去掉文件夹末尾的斜杠
                let filename = object.get_path().trim_end_matches('/').to_string();
                all_objects.push(ObjectInfo {
                    filename,
                    size: info.size(),
                    etag: info.etag().to_string(),
                    modified: *info.last_modified(),
                });
            }

            // 检查是否有下一页
            if current_objects.next_token().is_some() {
                // 如果有下一页，获取下一页数据
                current_objects = current_objects
                    .next_list(&condition, &client)
                    .await
                    .map_err(|e| format!("Failed to list next page of objects: {}", e))?;
            } else {
                // 没有下一页，跳出循环
                break;
            }
        }

        Ok(all_objects)
    }
}

impl ObjectInfo {
    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn etag(&self) -> &str {
        &self.etag
    }

    pub fn modified(&self) -> DateTime<Utc> {
        self.modified
    }
}
