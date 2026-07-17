use crate::tasks::TaskError;
use crate::utils::id_generate::generate_descending_id;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tauri::async_runtime::JoinHandle;

#[derive(Clone)]
pub struct TaskPool {
    // 任务句柄容器
    tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl TaskPool {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 接收一个异步任务并直接执行
    pub fn spawn<F>(&self, task: F) -> Result<String, TaskError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let cancel_token = generate_descending_id();
        let tasks_map = self.tasks.clone();
        let token_for_cleanup = cancel_token.clone();
        let token_for_return = cancel_token.clone();

        // 包装任务：先执行业务，再执行清理
        let wrapped_task = async move {
            task.await;
            if let Ok(mut guard) = tasks_map.lock() {
                guard.remove(&token_for_cleanup);
                #[cfg(debug_assertions)]
                println!("任务完成并移除: {}", token_for_cleanup);
            }
        };

        // 获取锁 (必须在 spawn 之前获取，防止竞态条件)
        let mut guard = self
            .tasks
            .lock()
            .map_err(|e| TaskError::LockPoisoned(e.to_string()))?;

        // 启动任务
        let handle = tauri::async_runtime::spawn(wrapped_task);

        // 存入句柄
        guard.insert(cancel_token, handle);

        Ok(token_for_return)
    }

    /// 取消任务
    pub fn cancel(&self, cancel_token: &str) -> Result<bool, TaskError> {
        let mut guard = self
            .tasks
            .lock()
            .map_err(|e| TaskError::LockPoisoned(e.to_string()))?;

        if let Some(handle) = guard.remove(cancel_token) {
            // 取消该任务
            handle.abort();
            Ok(true)
        } else {
            // 未找到对应的任务任务
            Ok(false)
        }
    }
}
