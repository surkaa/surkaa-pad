use super::diary::{get_diary_locked, lock_diary_operation};
use super::diary_types::{DiaryDetail, DiarySummary};
use super::draft::DiaryDraft;
use super::edit_lease::DiaryEditAccess;
use super::{DiaryContent, DiaryError};
use crate::{error::AppError, state::AppState};
use serde::Serialize;
use specta::Type;
use tauri::State;

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiaryEditView {
    detail: DiaryDetail,
    access: DiaryEditAccess,
    draft: Option<DiaryDraft>,
}

pub(crate) async fn open_edit_view(
    state: &AppState,
    id: &str,
    takeover: bool,
) -> Result<DiaryEditView, AppError> {
    let _storage_guard = state.lock_storage_operation().await?;
    let guard = lock_diary_operation(id).await;
    let store = state.diary_store();
    let cache = state.diary_cache();
    let draft = state.diary_drafts().read(id).await?;
    let manifest = match get_diary_locked(&cache, &state.crypto(), &*store, id, &guard).await {
        Ok(manifest) => manifest,
        Err(error) => {
            let Some(draft) = draft else {
                return Err(error.into());
            };
            // 云端已删除/暂时不可读时仍能打开、复制本机草稿，不虚构一个云端日记。
            let manifest = super::DiaryManifest {
                id: id.into(),
                algorithm: crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm,
                version: super::diary_types::CURRENT_VERSION,
                created: draft.saved_at,
                updated: draft.saved_at,
                content: draft.content.clone(),
                attachments: vec![],
            };
            return Ok(DiaryEditView {
                detail: DiaryDetail { summary: DiarySummary::from_manifest(&manifest), manifest_size: 0,
                    content: manifest.content, attachments: vec![], attachment_urls: Default::default() },
                access: DiaryEditAccess::read_only(format!("无法读取云端日记：{error}。当前显示本机草稿，可复制 Markdown 后另建日记。附件信息暂不可用。"), false),
                draft: Some(draft),
            });
        }
    };
    let baseline = cache
        .get(id)
        .ok_or_else(|| DiaryError::EditUnavailable("日记读取基线丢失".into()))?
        .1;
    let access = if state.is_remote_enabled() {
        state
            .diary_edits()
            .acquire(id, takeover, baseline)
            .await
            .unwrap_or_else(|error| {
                DiaryEditAccess::read_only(format!("无法确认云端编辑占用：{error}"), false)
            })
    } else {
        DiaryEditAccess::local()
    };
    let detail = DiaryDetail {
        summary: DiarySummary::from_manifest(&manifest),
        manifest_size: store.get_manifest_size(id).await?,
        attachment_urls: manifest
            .attachments
            .iter()
            .map(|att| (att.id.clone(), state.attachment_url(id, &att.id)))
            .collect(),
        attachments: manifest.attachments,
        content: manifest.content,
    };
    Ok(DiaryEditView {
        detail,
        access,
        draft,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_open_diary_edit(
    state: State<'_, AppState>,
    id: &str,
    takeover: bool,
) -> Result<DiaryEditView, AppError> {
    open_edit_view(&state, id, takeover).await
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_renew_diary_edit(
    state: State<'_, AppState>,
    id: &str,
    session_id: &str,
) -> Result<(), AppError> {
    let _storage_guard = state.lock_storage_operation().await?;
    if !state.is_remote_enabled() {
        return Err(DiaryError::EditUnavailable("存储模式已变化，请重新打开日记".into()).into());
    }
    Ok(state.diary_edits().renew(id, session_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_release_diary_edit(
    state: State<'_, AppState>,
    id: &str,
    session_id: &str,
) -> Result<(), AppError> {
    let _storage_guard = state.lock_storage_operation().await?;
    Ok(state.diary_edits().release(id, session_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_save_diary_draft(
    state: State<'_, AppState>,
    id: &str,
    content: DiaryContent,
) -> Result<String, AppError> {
    let _storage_guard = state.lock_storage_operation().await?;
    Ok(state.diary_drafts().save(id, content).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_discard_diary_draft(
    state: State<'_, AppState>,
    id: &str,
    revision: &str,
) -> Result<bool, AppError> {
    let _storage_guard = state.lock_storage_operation().await?;
    Ok(state.diary_drafts().clear_if_revision(id, revision).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_list_diary_draft_ids(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    let _storage_guard = state.lock_storage_operation().await?;
    Ok(state.diary_drafts().list_ids().await?)
}

#[cfg(test)]
#[path = "edit_command_tests.rs"]
mod tests;
