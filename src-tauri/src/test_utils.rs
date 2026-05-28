use crate::object::OssClient;
use std::sync::Mutex;

/// 存储上一个测试的 OssClient，下一个测试启动时接力清理。
/// Drop 中无法可靠执行 async，因此采用接力模式。
static PREV_CLIENT: Mutex<Option<OssClient>> = Mutex::new(None);

/// 测试用 OSS 守卫：确保每个测试都在干净的桶中运行。
///
/// - 创建时：清空桶内所有对象
/// - 销毁时：存储 client，供下一个测试接力清理
pub struct TestOssGuard {
    client: OssClient,
}

impl TestOssGuard {
    pub async fn new(client: OssClient) -> Self {
        // 接力清理上一测试遗留数据（同一次 cargo test 内）
        if let Ok(mut prev) = PREV_CLIENT.lock() {
            if let Some(prev_client) = prev.take() {
                let _ = prev_client.delete_with_prefix("").await;
            }
        }

        // 跨运行清理（新 cargo test 进程，PREV_CLIENT 为空）
        let _ = client.delete_with_prefix("").await;

        Self { client }
    }
}

impl Drop for TestOssGuard {
    fn drop(&mut self) {
        if let Ok(mut prev) = PREV_CLIENT.lock() {
            let _ = prev.insert(self.client.clone());
        }
    }
}
