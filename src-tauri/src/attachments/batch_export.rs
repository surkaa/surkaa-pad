use super::attachment_types::AttachmentProcessEvent;
use super::export_location::{completed_export, ExportLocation};
use super::{AttachmentError, AttachmentMeta};
use crate::diaries::get_diary;
use crate::error::AppError;
use crate::state::AppState;
use crate::stream::{tracker_stream, ByteStream};
use futures_util::StreamExt;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tauri::{ipc::Channel, AppHandle, State};
#[cfg(not(target_os = "android"))]
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::FilePath;
#[cfg(target_os = "android")]
use tauri_plugin_fs::FsExt;
use tauri_plugin_log::log;

fn export_error(message: impl ToString) -> AppError {
    AppError {
        error_type: "attachment_export".into(),
        message: message.to_string(),
    }
}

/// 只生成单级、跨平台安全的文件名；不能让附件展示名成为路径或 Windows 设备名。
fn safe_filename(source: &str) -> String {
    let name: String = source
        .chars()
        .map(|c| {
            if c.is_control() || ['<', '>', ':', '"', '/', '\\', '|', '?', '*'].contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut name = name.trim().trim_end_matches(['.', ' ']).to_owned();
    if name.is_empty() {
        name = "attachment".into();
    }
    let device = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    let port = device
        .strip_prefix("COM")
        .or_else(|| device.strip_prefix("LPT"));
    if ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&device.as_str())
        || port.is_some_and(|suffix| {
            suffix.chars().count() == 1 && suffix.chars().all(char::is_numeric)
        })
    {
        name.insert(0, '_');
    }
    // 留出自动编号空间，长文件名在字符边界截断，并尽量保留常用扩展名。
    if name.len() > 180 {
        let (stem, extension) = split_filename(&name);
        let mut extension = extension.to_string();
        while extension.len() > 32 {
            extension.pop();
        }
        let mut stem = stem.to_string();
        while stem.len() + extension.len() + 1 > 180 {
            stem.pop();
        }
        name = if extension.is_empty() {
            stem
        } else {
            format!("{stem}.{extension}")
        };
    }
    name
}

fn split_filename(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, extension),
        _ => (name, ""),
    }
}

fn numbered_filename(name: &str, index: usize) -> String {
    if index == 0 {
        return name.into();
    }
    let (stem, extension) = split_filename(name);
    if extension.is_empty() {
        format!("{stem} ({index})")
    } else {
        format!("{stem} ({index}).{extension}")
    }
}

/// 只删除本次新建且尚未成功提交的文件，绝不覆盖或删除用户原有文件。
struct PendingExportFile {
    file: Option<File>,
    destination: FilePath,
    committed: bool,
    #[cfg(target_os = "android")]
    app: AppHandle,
}

impl PendingExportFile {
    fn create(
        app: &AppHandle,
        directory: &FilePath,
        attachment: &AttachmentMeta,
    ) -> Result<Self, AttachmentError> {
        let name = safe_filename(&attachment.filename);
        match directory {
            FilePath::Path(path) => Self::create_local(path, &name, app),
            #[cfg(target_os = "android")]
            FilePath::Url(uri) => {
                let destination =
                    android::create_document(app, uri.as_str(), &name, &attachment.mimetype)?;
                let mut pending = Self {
                    file: None,
                    destination,
                    committed: false,
                    app: app.clone(),
                };
                let mut options = tauri_plugin_fs::OpenOptions::new();
                options.write(true).truncate(true);
                pending.file = Some(app.fs().open(pending.destination.clone(), options)?);
                Ok(pending)
            }
            #[cfg(not(target_os = "android"))]
            FilePath::Url(_) => Err(AttachmentError::InvalidOperation("不支持的保存目录".into())),
        }
    }

    fn create_local(
        directory: &Path,
        name: &str,
        _app: &AppHandle,
    ) -> Result<Self, AttachmentError> {
        let (file, path) = create_local_file(directory, name)?;
        Ok(Self {
            file: Some(file),
            destination: FilePath::Path(path),
            committed: false,
            #[cfg(target_os = "android")]
            app: _app.clone(),
        })
    }

    fn commit(mut self) -> Result<(), AttachmentError> {
        if let Some(mut file) = self.file.take() {
            file.flush()?;
        }
        self.committed = true;
        Ok(())
    }
}

fn create_local_file(
    directory: &Path,
    name: &str,
) -> Result<(File, std::path::PathBuf), AttachmentError> {
    for index in 0..100_000 {
        let path = directory.join(numbered_filename(name, index));
        // 同名目录同样需要避让；最终仍以 create_new 防止检查后的竞态覆盖。
        if path.try_exists()? {
            continue;
        }
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) if path.is_dir() => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(AttachmentError::InvalidOperation(
        "目录中同名文件过多".into(),
    ))
}

impl Drop for PendingExportFile {
    fn drop(&mut self) {
        // Windows 必须先关闭句柄，才能清理取消/失败产生的不完整文件。
        drop(self.file.take());
        if self.committed {
            return;
        }
        let result = match &self.destination {
            FilePath::Path(path) => std::fs::remove_file(path).map_err(|error| error.to_string()),
            #[cfg(target_os = "android")]
            FilePath::Url(uri) => {
                android::delete_document(&self.app, uri.as_str()).map_err(|error| error.to_string())
            }
            #[cfg(not(target_os = "android"))]
            FilePath::Url(_) => Ok(()),
        };
        if let Err(error) = result {
            log::warn!("清理未完成的附件导出失败：{error}");
        }
    }
}

async fn copy_stream(
    writer: &mut impl Write,
    mut stream: ByteStream,
) -> Result<u64, AttachmentError> {
    let mut written = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        writer.write_all(&chunk)?;
        written = written.saturating_add(chunk.len() as u64);
    }
    Ok(written)
}

fn selected_attachments(
    attachments: &[AttachmentMeta],
    ids: &[String],
) -> Result<Vec<AttachmentMeta>, AttachmentError> {
    if ids.is_empty() {
        return Err(AttachmentError::InvalidOperation("请先选择附件".into()));
    }
    let mut seen = HashSet::new();
    ids.iter()
        .filter(|id| seen.insert(id.as_str()))
        .map(|id| {
            attachments
                .iter()
                .find(|attachment| attachment.id == *id)
                .cloned()
                .ok_or_else(|| AttachmentError::InvalidOperation(format!("附件不存在：{id}")))
        })
        .collect()
}

fn batch_percent(
    done: u128,
    size: u64,
    progress: u8,
    total: u128,
    index: usize,
    count: usize,
) -> u8 {
    let weighted = done
        .saturating_mul(100)
        .saturating_add(u128::from(size) * u128::from(progress));
    let percent = weighted
        .checked_div(total)
        .unwrap_or_else(|| (index as u128 * 100 + u128::from(progress)) / count.max(1) as u128);
    percent.min(99) as u8 // 100% 只在所有文件都成功关闭后发送。
}

async fn export_batch(
    app: &AppHandle,
    state: &AppState,
    event: &Channel<AttachmentProcessEvent>,
    id: &str,
    ids: &[String],
    directory: &FilePath,
) -> Result<(), AttachmentError> {
    let store = state.diary_store();
    let crypto = state.crypto();
    // 目录选择期间可能发生改密或附件变动，拿到存储锁后重新读取最新元数据。
    let diary = get_diary(&state.diary_cache(), &crypto, &*store, id).await?;
    let attachments = selected_attachments(&diary.attachments, ids)?;
    let total: u128 = attachments
        .iter()
        .map(|attachment| u128::from(attachment.size))
        .sum();
    let last_progress = Arc::new(AtomicU8::new(0));
    let mut done = 0u128;
    let mut failures = Vec::new();
    let _ = event.send(AttachmentProcessEvent::Started);
    for (index, attachment) in attachments.iter().enumerate() {
        let result = async {
            let mut output = PendingExportFile::create(app, directory, attachment)?;
            let stream = store
                .download_attachment(id, &attachment.id, None, attachment.etag.as_deref())
                .await?;
            let (progress_event, previous, size, count) = (
                event.clone(),
                last_progress.clone(),
                attachment.size,
                attachments.len(),
            );
            let stream = tracker_stream(size, stream, move |progress| {
                let progress = batch_percent(done, size, progress, total, index, count);
                if previous.fetch_max(progress, Ordering::Relaxed) < progress {
                    let _ = progress_event.send(AttachmentProcessEvent::Progress(progress));
                }
            });
            let stream = if attachment.encrypted {
                crypto.decrypt_streaming(stream, &attachment.nonce, 0)?
            } else {
                stream
            };
            let written =
                copy_stream(output.file.as_mut().expect("新建文件必须有句柄"), stream).await?;
            if written != attachment.size {
                return Err(AttachmentError::InvalidOperation(format!(
                    "附件长度不匹配：预期 {}，实际 {written}",
                    attachment.size
                )));
            }
            output.commit()
        }
        .await;
        if let Err(error) = result {
            failures.push(format!("{}：{error}", attachment.filename));
        }
        done += u128::from(attachment.size);
    }
    if !failures.is_empty() {
        return Err(AttachmentError::InvalidOperation(format!(
            "已保存 {} 个，失败 {} 个。{}",
            attachments.len() - failures.len(),
            failures.len(),
            failures
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join("；")
        )));
    }
    let _ = event.send(AttachmentProcessEvent::Progress(100));
    Ok(())
}

/// 选择一次目录，逐个流式导出解密后的原文件；整个批次可取消，已完成文件保留。
#[tauri::command]
#[specta::specta]
pub async fn cmd_save_decrypt_attachments(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    event: Channel<AttachmentProcessEvent>,
    id: String,
    attachment_ids: Vec<String>,
) -> Result<Option<String>, AppError> {
    {
        let _guard = state.lock_storage_operation().await?;
        let diary = get_diary(
            &state.diary_cache(),
            &state.crypto(),
            &*state.diary_store(),
            &id,
        )
        .await?;
        selected_attachments(&diary.attachments, &attachment_ids)?;
    }
    #[cfg(not(target_os = "android"))]
    let directory = app_handle
        .dialog()
        .file()
        .set_title("选择附件保存目录")
        .blocking_pick_folder();
    #[cfg(target_os = "android")]
    let directory = android::pick_directory(&app_handle)?;
    let Some(directory) = directory else {
        return Ok(None);
    };
    let pool = state.task_pool();
    let state = state.inner().clone();
    Ok(Some(pool.spawn_cancelable(move |cancel| async move {
        let work = async {
            let _guard = state.lock_storage_operation().await.map_err(export_error)?;
            export_batch(
                &app_handle,
                &state,
                &event,
                &id,
                &attachment_ids,
                &directory,
            )
            .await
            .map_err(export_error)
        };
        let result = tokio::select! {
            biased;
            () = cancel.cancelled() => Err(export_error("批量下载已取消；已完成文件保留")),
            result = work => result,
        };
        let _ = event.send(match result {
            Ok(()) => completed_export(
                &app_handle,
                ExportLocation {
                    path: directory,
                    directory: true,
                },
            ),
            Err(error) => AttachmentProcessEvent::Error(error.to_string()),
        });
    })))
}

#[cfg(target_os = "android")]
pub(crate) mod android {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tauri::{
        plugin::{Builder, PluginHandle, TauriPlugin},
        Manager, Runtime,
    };
    struct AndroidExport<R: Runtime>(PluginHandle<R>);
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct CreateFile<'a> {
        directory_uri: &'a str,
        filename: &'a str,
        mimetype: &'a str,
    }
    #[derive(Deserialize)]
    struct CreatedFile {
        uri: String,
    }
    #[derive(Deserialize)]
    struct SelectedDirectory {
        uri: Option<String>,
    }
    pub(super) fn pick_directory(app: &AppHandle) -> Result<Option<FilePath>, AttachmentError> {
        let result: SelectedDirectory = app
            .state::<AndroidExport<tauri::Wry>>()
            .inner()
            .0
            .run_mobile_plugin("pickDirectory", serde_json::json!({}))
            .map_err(|error| AttachmentError::FileOperationFailed(error.to_string()))?;
        result
            .uri
            .map(|uri| {
                uri.parse().map(FilePath::Url).map_err(|error| {
                    AttachmentError::FileOperationFailed(format!(
                        "系统返回了无效的目录地址：{error}"
                    ))
                })
            })
            .transpose()
    }
    #[derive(Serialize)]
    struct DeleteFile<'a> {
        uri: &'a str,
    }
    pub(super) fn create_document(
        app: &AppHandle,
        directory: &str,
        filename: &str,
        mimetype: &str,
    ) -> Result<FilePath, AttachmentError> {
        let result: CreatedFile = app
            .state::<AndroidExport<tauri::Wry>>()
            .inner()
            .0
            .run_mobile_plugin(
                "createFile",
                CreateFile {
                    directory_uri: directory,
                    filename,
                    mimetype,
                },
            )
            .map_err(|error| AttachmentError::FileOperationFailed(error.to_string()))?;
        result.uri.parse().map(FilePath::Url).map_err(|error| {
            AttachmentError::FileOperationFailed(format!("系统返回了无效的文件地址：{error}"))
        })
    }
    pub(super) fn delete_document(app: &AppHandle, uri: &str) -> Result<(), AttachmentError> {
        let _: serde_json::Value = app
            .state::<AndroidExport<tauri::Wry>>()
            .inner()
            .0
            .run_mobile_plugin("deleteFile", DeleteFile { uri })
            .map_err(|error| AttachmentError::FileOperationFailed(error.to_string()))?;
        Ok(())
    }
    pub(crate) fn open_location(
        app: &AppHandle,
        uri: &str,
        directory: bool,
    ) -> Result<(), AttachmentError> {
        let _: serde_json::Value = app
            .state::<AndroidExport<tauri::Wry>>()
            .inner()
            .0
            .run_mobile_plugin(
                "openLocation",
                serde_json::json!({"uri": uri, "directory": directory}),
            )
            .map_err(|error| AttachmentError::FileOperationFailed(error.to_string()))?;
        Ok(())
    }
    pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
        Builder::new("attachment-export")
            .setup(|app, api| {
                let handle = api.register_android_plugin(
                    "cn.surkaa.pad.attachments",
                    "AttachmentExportPlugin",
                )?;
                app.manage(AndroidExport(handle));
                Ok(())
            })
            .build()
    }
}

#[cfg(test)]
#[path = "batch_export_tests.rs"]
mod tests;
