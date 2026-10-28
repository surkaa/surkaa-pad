use super::attachment_types::AttachmentProcessEvent;
use crate::error::AppError;
use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_fs::FilePath;
#[cfg(not(target_os = "android"))]
use tauri_plugin_opener::OpenerExt;

const MAX_EXPORT_LOCATIONS: usize = 64;

#[derive(Clone)]
pub(crate) struct ExportLocation {
    pub path: FilePath,
    pub directory: bool,
}

/// 只在本次运行中记录已成功导出的真实位置。前端仅取得随机句柄，不能指定任意路径。
#[derive(Default)]
pub(crate) struct ExportLocations(Mutex<VecDeque<(String, ExportLocation)>>);

impl ExportLocations {
    fn register(&self, location: ExportLocation) -> Result<String, AppError> {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(location_error)?;
        let token = hex::encode(random);
        let mut locations = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if locations.len() >= MAX_EXPORT_LOCATIONS {
            locations.pop_front();
        }
        locations.push_back((token.clone(), location));
        Ok(token)
    }

    fn get(&self, token: &str) -> Result<ExportLocation, AppError> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .find(|(key, _)| key == token)
            .map(|(_, location)| location.clone())
            .ok_or_else(|| location_error("下载位置已失效，请在文件管理器中打开保存目录"))
    }
}

fn location_error(message: impl ToString) -> AppError {
    AppError {
        error_type: "attachment_download_location".into(),
        message: message.to_string(),
    }
}

pub(crate) fn completed_export(
    app: &AppHandle,
    location: ExportLocation,
) -> AttachmentProcessEvent {
    match app.state::<ExportLocations>().register(location) {
        Ok(token) => AttachmentProcessEvent::CompletedWithDownloadLocation(token),
        Err(error) => {
            // 文件已经保存；附加的目录快捷操作失败不能误报下载失败。
            tauri_plugin_log::log::warn!("记录下载位置失败：{error}");
            AttachmentProcessEvent::CompletedWithoutData
        }
    }
}

#[cfg(not(target_os = "android"))]
fn local_directory(location: &ExportLocation) -> Result<std::path::PathBuf, AppError> {
    let FilePath::Path(path) = &location.path else {
        return Err(location_error("无法打开此下载位置"));
    };
    if location.directory {
        return Ok(path.clone());
    }
    path.parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| location_error("无法获取附件保存目录"))
}

/// 仅接受本进程成功下载后产生的句柄，不接收前端文件路径或 URL。
#[tauri::command]
#[specta::specta]
pub async fn cmd_open_attachment_download_location(
    app: AppHandle,
    locations: State<'_, ExportLocations>,
    token: String,
) -> Result<(), AppError> {
    let location = locations.get(&token)?;
    #[cfg(not(target_os = "android"))]
    {
        let directory = local_directory(&location)?;
        if !directory.is_dir() {
            return Err(location_error("保存目录不存在或已被移动"));
        }
        app.opener()
            .open_path(directory.to_string_lossy().into_owned(), None::<&str>)
            .map_err(location_error)
    }
    #[cfg(target_os = "android")]
    {
        let FilePath::Url(uri) = location.path else {
            return Err(location_error(
                "系统无法直接打开此下载位置，请使用文件管理器查看",
            ));
        };
        super::batch_export::android::open_location(&app, uri.as_str(), location.directory)
            .map_err(location_error)
    }
}

#[cfg(test)]
#[path = "export_location_tests.rs"]
mod tests;
