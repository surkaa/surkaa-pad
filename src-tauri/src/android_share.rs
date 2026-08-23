use crate::error::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

#[cfg(target_os = "android")]
use tauri::{
    plugin::{Builder, PluginHandle, TauriPlugin},
    Manager, Runtime,
};

#[cfg(target_os = "android")]
const ANDROID_PLUGIN_IDENTIFIER: &str = "cn.surkaa.pad.sharetarget";

/// Android 分享面板交给应用、但尚未导入日记的一批内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PendingAndroidShare {
    pub id: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    pub items: Vec<PendingAndroidShareItem>,
}

/// 分享批次中的一个文件。`uri` 仅作为 Android 临时授权的读取入口，不会被持久化。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PendingAndroidShareItem {
    pub id: String,
    pub uri: String,
    pub display_name: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    #[specta(type = Option<f64>)]
    pub size: Option<u64>,
}

/// 查看尚未导入的 Android 系统分享内容。
/// Windows 端始终返回空数组。读取不会消费队列；成功导入或明确放弃后需调用
/// [`cmd_ack_pending_android_share`]。
#[tauri::command]
#[specta::specta]
pub fn cmd_list_pending_android_shares(
    app: AppHandle,
) -> Result<Vec<PendingAndroidShare>, AppError> {
    #[cfg(target_os = "android")]
    {
        return app
            .android_share_inbox()
            .list_pending()
            .map(|response| response.batches)
            .map_err(plugin_error);
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(Vec::new())
    }
}

/// 确认某批 Android 系统分享已完成导入或已被用户明确放弃。
/// 该操作是幂等的，批次已经不存在时也会成功返回。
#[tauri::command]
#[specta::specta]
pub fn cmd_ack_pending_android_share(app: AppHandle, batch_id: String) -> Result<(), AppError> {
    validate_batch_id(&batch_id)?;

    #[cfg(target_os = "android")]
    {
        let response = app
            .android_share_inbox()
            .ack_pending(&batch_id)
            .map_err(plugin_error)?;
        let _ = response.acknowledged;
    }

    #[cfg(not(target_os = "android"))]
    let _ = app;

    Ok(())
}

fn validate_batch_id(batch_id: &str) -> Result<(), AppError> {
    if batch_id.trim().is_empty() {
        return Err(AppError {
            error_type: "invalid_android_share".into(),
            message: "分享批次 ID 不能为空".into(),
        });
    }
    Ok(())
}

#[cfg(target_os = "android")]
fn plugin_error(error: tauri::plugin::mobile::PluginInvokeError) -> AppError {
    AppError {
        error_type: "android_share_target".into(),
        message: error.to_string(),
    }
}

#[cfg(target_os = "android")]
#[derive(Serialize)]
struct EmptyRequest {}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListPendingResponse {
    #[serde(default)]
    batches: Vec<PendingAndroidShare>,
}

#[cfg(target_os = "android")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AckPendingRequest<'a> {
    batch_id: &'a str,
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
struct AckPendingResponse {
    acknowledged: bool,
}

#[cfg(target_os = "android")]
struct AndroidShareInbox<R: Runtime>(PluginHandle<R>);

#[cfg(target_os = "android")]
impl<R: Runtime> AndroidShareInbox<R> {
    fn list_pending(
        &self,
    ) -> Result<ListPendingResponse, tauri::plugin::mobile::PluginInvokeError> {
        self.0
            .run_mobile_plugin("listPendingShares", EmptyRequest {})
    }

    fn ack_pending(
        &self,
        batch_id: &str,
    ) -> Result<AckPendingResponse, tauri::plugin::mobile::PluginInvokeError> {
        self.0
            .run_mobile_plugin("ackPendingShare", AckPendingRequest { batch_id })
    }
}

#[cfg(target_os = "android")]
trait AndroidShareInboxExt<R: Runtime> {
    fn android_share_inbox(&self) -> &AndroidShareInbox<R>;
}

#[cfg(target_os = "android")]
impl<R: Runtime, T: Manager<R>> AndroidShareInboxExt<R> for T {
    fn android_share_inbox(&self) -> &AndroidShareInbox<R> {
        self.state::<AndroidShareInbox<R>>().inner()
    }
}

#[cfg(target_os = "android")]
pub fn init_android_plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-share-target")
        .setup(|app, api| {
            let handle =
                api.register_android_plugin(ANDROID_PLUGIN_IDENTIFIER, "ShareTargetPlugin")?;
            app.manage(AndroidShareInbox(handle));
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::validate_batch_id;

    #[test]
    fn rejects_blank_batch_ids() {
        assert!(validate_batch_id("").is_err());
        assert!(validate_batch_id("   ").is_err());
        assert!(validate_batch_id("share-id").is_ok());
    }
}
