use crate::error::AppError;
use crate::state::AppState;
use tauri::State;

/// 取消任务
#[tauri::command]
#[specta::specta]
pub fn cmd_cancel_task(
    state: State<'_, AppState>,
    cancel_token: &str,
) -> Result<bool, AppError> {
    Ok(state.task_pool().cancel(cancel_token)?)
}
