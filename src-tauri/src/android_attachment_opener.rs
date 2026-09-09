use crate::error::AppError;
use std::path::Path;
use tauri::AppHandle;

#[cfg(target_os = "android")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "android")]
use tauri::{
    plugin::{Builder, PluginHandle, TauriPlugin},
    Manager, Runtime,
};

#[cfg(target_os = "android")]
const ANDROID_PLUGIN_IDENTIFIER: &str = "cn.surkaa.pad.attachmentopener";

pub fn open_external_file(
    app: &AppHandle,
    path: &Path,
    mime_type: &str,
    display_name: &str,
) -> Result<(), AppError> {
    #[cfg(target_os = "android")]
    {
        let path = path.to_str().ok_or_else(|| AppError {
            error_type: "open_attachment".into(),
            message: "临时附件路径不是有效的 UTF-8".into(),
        })?;
        let response = app
            .android_attachment_opener()
            .open(path, mime_type, display_name)
            .map_err(|error| AppError {
                error_type: "open_attachment".into(),
                message: error.to_string(),
            })?;
        if !response.opened {
            return Err(AppError {
                error_type: "open_attachment".into(),
                message: "系统未能打开 HTML 附件".into(),
            });
        }
        Ok(())
    }

    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;

        let _ = (mime_type, display_name);
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|error| AppError {
                error_type: "open_attachment".into(),
                message: error.to_string(),
            })
    }
}

#[cfg(target_os = "android")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenAttachmentRequest<'a> {
    path: &'a str,
    mime_type: &'a str,
    display_name: &'a str,
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
struct OpenAttachmentResponse {
    opened: bool,
}

#[cfg(target_os = "android")]
struct AndroidAttachmentOpener<R: Runtime>(PluginHandle<R>);

#[cfg(target_os = "android")]
impl<R: Runtime> AndroidAttachmentOpener<R> {
    fn open(
        &self,
        path: &str,
        mime_type: &str,
        display_name: &str,
    ) -> Result<OpenAttachmentResponse, tauri::plugin::mobile::PluginInvokeError> {
        self.0.run_mobile_plugin(
            "openAttachment",
            OpenAttachmentRequest {
                path,
                mime_type,
                display_name,
            },
        )
    }
}

#[cfg(target_os = "android")]
trait AndroidAttachmentOpenerExt<R: Runtime> {
    fn android_attachment_opener(&self) -> &AndroidAttachmentOpener<R>;
}

#[cfg(target_os = "android")]
impl<R: Runtime, T: Manager<R>> AndroidAttachmentOpenerExt<R> for T {
    fn android_attachment_opener(&self) -> &AndroidAttachmentOpener<R> {
        self.state::<AndroidAttachmentOpener<R>>().inner()
    }
}

#[cfg(target_os = "android")]
pub fn init_android_plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-attachment-opener")
        .setup(|app, api| {
            let handle =
                api.register_android_plugin(ANDROID_PLUGIN_IDENTIFIER, "AttachmentOpenerPlugin")?;
            app.manage(AndroidAttachmentOpener(handle));
            Ok(())
        })
        .build()
}
