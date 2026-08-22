use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::collections::HashSet;
use thiserror::Error;

pub const CURRENT_SYNCED_SETTINGS_VERSION: u32 = 1;
const MAX_PINNED_DIARIES: usize = 100_000;
const MAX_SHORTCUT_CHARS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum SyncedTheme {
    Light,
    Dark,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum EditorToolbarAction {
    Bold,
    Underline,
    Strike,
    Heading1,
    Heading2,
    Heading3,
    TaskList,
    Summary,
}

impl EditorToolbarAction {
    const ALL: [Self; 8] = [
        Self::Bold,
        Self::Underline,
        Self::Strike,
        Self::Heading1,
        Self::Heading2,
        Self::Heading3,
        Self::TaskList,
        Self::Summary,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncedAppearanceSettings {
    pub theme: SyncedTheme,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentSettings {
    pub default_image_size_is_small: bool,
    pub encrypt_image_attachments: bool,
    pub encrypt_audio_attachments: bool,
    pub encrypt_video_attachments: bool,
    pub encrypt_file_attachments: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditorShortcutSettings {
    pub insert_photo: String,
    pub insert_audio: String,
    pub audio_recording: String,
    pub insert_video: String,
    pub insert_file: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiaryListShortcutSettings {
    pub create_diary: String,
    pub ai_assistant: String,
    pub search: String,
    pub settings: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiAssistantShortcutSettings {
    pub focus_input: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WindowsSettings {
    pub editor_shortcuts: EditorShortcutSettings,
    pub diary_list_shortcuts: DiaryListShortcutSettings,
    pub ai_assistant_shortcuts: AiAssistantShortcutSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditorSettings {
    pub toolbar_order: Vec<EditorToolbarAction>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncedSettingsData {
    pub appearance: SyncedAppearanceSettings,
    pub attachments: AttachmentSettings,
    pub editor: EditorSettings,
    pub pinned_diary_ids: Vec<String>,
    pub windows: WindowsSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncedSettingsDocument {
    pub version: u32,
    #[specta(type = f64)]
    pub updated_at: i64,
    #[serde(flatten)]
    pub data: SyncedSettingsData,
}

#[derive(Debug, Error)]
pub enum SyncedSettingsError {
    #[error("同步设置不是有效的 JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("同步设置数据无效: {0}")]
    InvalidData(String),
    #[error("不支持同步设置版本 V{found}，当前仅支持 V{supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
}

impl SyncedSettingsData {
    pub fn validate(&self) -> Result<(), SyncedSettingsError> {
        if self.pinned_diary_ids.len() > MAX_PINNED_DIARIES {
            return Err(SyncedSettingsError::InvalidData(format!(
                "置顶日记数量不能超过 {MAX_PINNED_DIARIES}"
            )));
        }
        let mut diary_ids = HashSet::new();
        for diary_id in &self.pinned_diary_ids {
            if diary_id.is_empty() || !diary_id.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(SyncedSettingsError::InvalidData(format!(
                    "置顶日记 ID {diary_id:?} 不是数字"
                )));
            }
            if !diary_ids.insert(diary_id) {
                return Err(SyncedSettingsError::InvalidData(format!(
                    "置顶日记 ID {diary_id} 重复"
                )));
            }
        }

        let toolbar_actions = self
            .editor
            .toolbar_order
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        if self.editor.toolbar_order.len() != EditorToolbarAction::ALL.len()
            || toolbar_actions.len() != EditorToolbarAction::ALL.len()
            || !EditorToolbarAction::ALL
                .iter()
                .all(|action| toolbar_actions.contains(action))
        {
            return Err(SyncedSettingsError::InvalidData(
                "编辑器工具栏顺序必须完整且不能重复".into(),
            ));
        }

        for (name, shortcut) in self.shortcuts() {
            if shortcut.chars().count() > MAX_SHORTCUT_CHARS {
                return Err(SyncedSettingsError::InvalidData(format!(
                    "快捷键 {name} 不能超过 {MAX_SHORTCUT_CHARS} 个字符"
                )));
            }
        }
        Ok(())
    }

    fn shortcuts(&self) -> [(&'static str, &str); 10] {
        let editor = &self.windows.editor_shortcuts;
        let diary_list = &self.windows.diary_list_shortcuts;
        [
            ("insertPhoto", &editor.insert_photo),
            ("insertAudio", &editor.insert_audio),
            ("audioRecording", &editor.audio_recording),
            ("insertVideo", &editor.insert_video),
            ("insertFile", &editor.insert_file),
            ("createDiary", &diary_list.create_diary),
            ("aiAssistant", &diary_list.ai_assistant),
            ("search", &diary_list.search),
            ("settings", &diary_list.settings),
            (
                "focusInput",
                &self.windows.ai_assistant_shortcuts.focus_input,
            ),
        ]
    }
}

pub fn deserialize_synced_settings(
    bytes: &[u8],
) -> Result<SyncedSettingsDocument, SyncedSettingsError> {
    let value: Value = serde_json::from_slice(bytes)?;
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .ok_or_else(|| SyncedSettingsError::InvalidData("缺少有效的 version 字段".into()))?;
    if version != CURRENT_SYNCED_SETTINGS_VERSION {
        return Err(SyncedSettingsError::UnsupportedVersion {
            found: version,
            supported: CURRENT_SYNCED_SETTINGS_VERSION,
        });
    }
    let document: SyncedSettingsDocument = serde_json::from_value(value)?;
    if document.updated_at < 0 {
        return Err(SyncedSettingsError::InvalidData(
            "更新时间不能为负数".into(),
        ));
    }
    document.data.validate()?;
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn sample_settings() -> SyncedSettingsData {
        SyncedSettingsData {
            appearance: SyncedAppearanceSettings {
                theme: SyncedTheme::Dark,
            },
            attachments: AttachmentSettings {
                default_image_size_is_small: true,
                encrypt_image_attachments: true,
                encrypt_audio_attachments: false,
                encrypt_video_attachments: true,
                encrypt_file_attachments: false,
            },
            editor: EditorSettings {
                toolbar_order: EditorToolbarAction::ALL.to_vec(),
            },
            pinned_diary_ids: vec!["8215021834823".into()],
            windows: WindowsSettings {
                editor_shortcuts: EditorShortcutSettings {
                    insert_photo: "Ctrl+Alt+KeyP".into(),
                    insert_audio: "Ctrl+Alt+KeyA".into(),
                    audio_recording: "Ctrl+Alt+KeyR".into(),
                    insert_video: "Ctrl+Alt+KeyV".into(),
                    insert_file: "Ctrl+Alt+KeyF".into(),
                },
                diary_list_shortcuts: DiaryListShortcutSettings {
                    create_diary: "Ctrl+KeyN".into(),
                    ai_assistant: "Ctrl+Alt+KeyA".into(),
                    search: "Ctrl+KeyF".into(),
                    settings: "Ctrl+Comma".into(),
                },
                ai_assistant_shortcuts: AiAssistantShortcutSettings {
                    focus_input: "Ctrl+Alt+KeyI".into(),
                },
            },
        }
    }

    #[test]
    fn serializes_stable_camel_case_document() {
        let document = SyncedSettingsDocument {
            version: CURRENT_SYNCED_SETTINGS_VERSION,
            updated_at: 123,
            data: sample_settings(),
        };
        let value = serde_json::to_value(&document).unwrap();

        assert_eq!(value["version"], 1);
        assert_eq!(value["updatedAt"], 123);
        assert_eq!(value["appearance"]["theme"], "dark");
        assert_eq!(value["editor"]["toolbarOrder"][6], "taskList");
        assert_eq!(
            value["windows"]["editorShortcuts"]["insertPhoto"],
            "Ctrl+Alt+KeyP"
        );
    }

    #[test]
    fn rejects_unsupported_versions_and_invalid_data() {
        let mut value = serde_json::to_value(SyncedSettingsDocument {
            version: CURRENT_SYNCED_SETTINGS_VERSION,
            updated_at: 123,
            data: sample_settings(),
        })
        .unwrap();
        value["version"] = 2.into();
        assert!(matches!(
            deserialize_synced_settings(&serde_json::to_vec(&value).unwrap()),
            Err(SyncedSettingsError::UnsupportedVersion {
                found: 2,
                supported: 1
            })
        ));

        let mut invalid = sample_settings();
        invalid.pinned_diary_ids.push("not-a-diary".into());
        assert!(matches!(
            invalid.validate(),
            Err(SyncedSettingsError::InvalidData(_))
        ));
        let mut invalid = sample_settings();
        invalid.editor.toolbar_order.pop();
        assert!(matches!(
            invalid.validate(),
            Err(SyncedSettingsError::InvalidData(_))
        ));
    }
}
