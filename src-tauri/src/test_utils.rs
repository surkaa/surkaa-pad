use crate::object::OssClient;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// 为每个测试分配独立 OSS 前缀；成功时清理，panic 时保留现场。
pub struct TestOssGuard {
    client: OssClient,
    prefix: String,
    cleaned: bool,
}

impl TestOssGuard {
    pub async fn new(client: OssClient) -> (OssClient, Self) {
        let prefix = unique_test_prefix();
        println!("[OSS TEST] prefix={prefix}");
        let client = client.with_key_prefix(prefix.clone());
        let guard = Self {
            client: client.clone(),
            prefix,
            cleaned: false,
        };
        (client, guard)
    }

    pub async fn cleanup(mut self) {
        match self.client.delete_with_prefix("").await {
            Ok(keys) => {
                self.cleaned = true;
                println!(
                    "[OSS TEST] passed; cleaned prefix={}, objects={}",
                    self.prefix,
                    keys.len()
                );
            }
            Err(error) => eprintln!(
                "[OSS TEST] cleanup failed; retained prefix={}, error={error}",
                self.prefix
            ),
        }
    }
}

impl Drop for TestOssGuard {
    fn drop(&mut self) {
        if !self.cleaned {
            eprintln!("[OSS TEST] retained prefix={}", self.prefix);
        }
    }
}

fn unique_test_prefix() -> String {
    let test_name = std::thread::current()
        .name()
        .unwrap_or("unknown-test")
        .to_string();
    let safe_name: String = test_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "rust-tests/{safe_name}/{}-{timestamp}-{sequence}",
        std::process::id()
    )
}

#[cfg(test)]
mod tests {
    use super::unique_test_prefix;

    #[test]
    fn generated_prefixes_are_unique_and_safe() {
        let first = unique_test_prefix();
        let second = unique_test_prefix();
        assert_ne!(first, second);
        assert!(first.starts_with("rust-tests/"));
        assert!(!first.contains("::"));
        assert!(!first.ends_with('/'));
    }
}
