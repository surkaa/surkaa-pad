use crate::utils::id_generate::generate_descending_id;
use dashmap::DashMap;
use futures_util::FutureExt;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::async_runtime::JoinHandle;
use tauri_plugin_log::log;
use tokio::sync::oneshot;

static TASK_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct TaskPool {
    tasks: Arc<DashMap<String, JoinHandle<()>>>,
}

impl TaskPool {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(DashMap::new()),
        }
    }

    /// 接收一个异步任务并直接执行
    pub fn spawn<F>(&self, task: F) -> String
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let sequence = TASK_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let cancel_token = format!("{}-{sequence}", generate_descending_id());
        let tasks_map = self.tasks.clone();
        let token_for_cleanup = cancel_token.clone();
        let (registered_tx, registered_rx) = oneshot::channel();

        // 等句柄注册后再执行业务，避免极快任务先清理、后插入的竞态。
        let wrapped_task = async move {
            if registered_rx.await.is_err() {
                return;
            }
            let outcome = AssertUnwindSafe(task).catch_unwind().await;
            tasks_map.remove(&token_for_cleanup);
            match outcome {
                Ok(()) => log::debug!("后台任务完成并移除: {token_for_cleanup}"),
                Err(payload) => {
                    let message = payload
                        .downcast_ref::<&str>()
                        .copied()
                        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                        .unwrap_or("未知 panic");
                    log::error!(
                        "后台任务发生 panic，已隔离并清理: token={token_for_cleanup}, error={message}"
                    );
                }
            }
        };

        let handle = tauri::async_runtime::spawn(wrapped_task);
        self.tasks.insert(cancel_token.clone(), handle);
        let _ = registered_tx.send(());
        cancel_token
    }

    /// 取消任务
    pub fn cancel(&self, cancel_token: &str) -> bool {
        if let Some((_, handle)) = self.tasks.remove(cancel_token) {
            handle.abort();
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    fn task_count(&self) -> usize {
        self.tasks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use tokio::time::{timeout, Duration};

    async fn wait_until_empty(pool: &TaskPool) {
        timeout(Duration::from_secs(1), async {
            while pool.task_count() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("任务句柄未及时清理");
    }

    #[tokio::test]
    async fn completed_and_panicking_tasks_are_cleaned_up() {
        let pool = TaskPool::new();
        pool.spawn(async {});
        wait_until_empty(&pool).await;

        pool.spawn(async { panic!("simulated task panic") });
        wait_until_empty(&pool).await;

        let completed = Arc::new(AtomicBool::new(false));
        let completed_in_task = completed.clone();
        pool.spawn(async move {
            completed_in_task.store(true, Ordering::Relaxed);
        });
        wait_until_empty(&pool).await;
        assert!(completed.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn tokens_are_unique_and_tasks_can_be_cancelled() {
        let pool = TaskPool::new();
        let first = pool.spawn(std::future::pending());
        let second = pool.spawn(std::future::pending());

        assert_ne!(first, second);
        assert!(pool.cancel(&first));
        assert!(!pool.cancel(&first));
        assert!(pool.cancel(&second));
        assert_eq!(pool.task_count(), 0);
    }
}
