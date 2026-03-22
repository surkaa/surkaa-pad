use crate::state::AppState;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn cmd_clean_cache_file(state: State<'_, AppState>) -> Result<(), String> {
    state.local_file_cache().delete_all().await
}
