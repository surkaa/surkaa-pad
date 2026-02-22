use crate::object::OssClient;
use std::sync::{Arc, OnceLock};

#[derive(Clone)]
pub struct OssState(Arc<OnceLock<OssClient>>);

impl OssState {
    pub fn new() -> Self {
        Self(Arc::new(OnceLock::new()))
    }

    pub async fn initialize(
        &self,
        akid: String,
        sakey: String,
        endpoint: String,
        bucket: String,
    ) -> Result<(), String> {
        // 创建 OssClient
        let client = OssClient::new(endpoint, akid, sakey, bucket);
        // 测试 client 是否可用
        let _ = client.list("", Some(1), None).await?;
        // 存储 client
        self.0
            .set(client)
            .map_err(|_| String::from("OssClient 已初始化"))?;
        Ok(())
    }

    pub fn get_client(&self) -> Result<OssClient, String> {
        self.0.get().cloned().ok_or(String::from("OssClient 未初始化"))
    }
}
