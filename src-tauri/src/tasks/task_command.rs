use crate::error::AppError;
use crate::state::AppState;
use tauri::State;

/// 取消任务
/// # Arguments
/// * `cancel_token` - 创建后台任务时返回的取消令牌
/// # Returns
/// * `Result<bool, AppError>` - `true` 表示找到并取消了任务，`false` 表示任务已不存在
#[tauri::command]
#[specta::specta]
pub fn cmd_cancel_task(state: State<'_, AppState>, cancel_token: &str) -> Result<bool, AppError> {
    Ok(state.task_pool().cancel(cancel_token))
}
