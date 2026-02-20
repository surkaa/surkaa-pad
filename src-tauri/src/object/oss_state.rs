use std::sync::OnceLock;
use tauri::State;
use crate::object::OssClient;

pub struct OssState(OnceLock<OssClient>);

impl OssState {
    pub fn new() -> Self {
        Self(OnceLock::new())
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
        let _ = client.list("", None).await?;
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


/// 初始化 OSS 客户端
/// # Arguments
/// * `akid` - 访问密钥 ID
/// * `aks` - 访问密钥 Secret
/// * `bucket` - 存储桶名称
/// * `endpoint` - OSS 端点
/// # Returns
/// * `Result<(), String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub async fn init_oss_client(
    client_state: State<'_, OssState>,
    akid: String,
    aks: String,
    bucket: String,
    endpoint: String,
) -> Result<(), String> {
    client_state
        .initialize(akid, aks, endpoint, bucket)
        .await
        .map_err(|e| e.to_string())
}
