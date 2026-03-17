use crate::state::AppState;
use tauri::State;

/// 取消任务
/// # Arguments
/// * `cancel_token` - 任务取消令牌
/// # Returns
/// * `Result<bool, String>` - 成功时返回 Ok，失败时返回错误信息
#[tauri::command]
#[specta::specta]
pub fn cmd_cancel_task(state: State<'_, AppState>, cancel_token: &str) -> Result<bool, String> {
    state.task_pool().cancel(cancel_token)
}
