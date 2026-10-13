use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::collections::HashSet;
use thiserror::Error;

pub const CURRENT_SYNCED_SETTINGS_VERSION: u32 = 4;
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
    Heading4,
    Heading5,
    BulletList,
    TaskList,
    Summary,
    Undo,
    Redo,
}

impl EditorToolbarAction {
    const ALL: [Self; 13] = [
        Self::Bold,
        Self::Underline,
        Self::Strike,
        Self::Heading1,
        Self::Heading2,
        Self::Heading3,
        Self::Heading4,
        Self::Heading5,
        Self::BulletList,
        Self::TaskList,
        Self::Summary,
        Self::Undo,
        Self::Redo,
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
#[serde(rename_all = "camelCase", default)]
pub struct EditorShortcutSettings {
    pub bold: String,
    pub underline: String,
    pub strike: String,
    pub heading1: String,
    pub heading2: String,
    pub heading3: String,
    pub heading4: String,
    pub heading5: String,
    pub bullet_list: String,
    pub task_list: String,
    pub summary: String,
    pub insert_photo: String,
    pub insert_audio: String,
    pub audio_recording: String,
    pub insert_video: String,
    pub insert_file: String,
}

impl Default for EditorShortcutSettings {
    fn default() -> Self {
        Self {
            bold: "Ctrl+KeyB".into(),
            underline: "Ctrl+KeyU".into(),
            strike: "Ctrl+Shift+KeyS".into(),
            heading1: "Ctrl+Digit1".into(),
            heading2: "Ctrl+Digit2".into(),
            heading3: "Ctrl+Digit3".into(),
            heading4: "Ctrl+Digit4".into(),
            heading5: "Ctrl+Digit5".into(),
            bullet_list: "Ctrl+Shift+Digit8".into(),
            task_list: "Ctrl+KeyT".into(),
            summary: "Ctrl+Alt+KeyS".into(),
            insert_photo: "Ctrl+Alt+KeyP".into(),
            insert_audio: "Ctrl+Alt+KeyA".into(),
            audio_recording: "Ctrl+Alt+KeyR".into(),
            insert_video: "Ctrl+Alt+KeyV".into(),
            insert_file: "Ctrl+Alt+KeyF".into(),
        }
    }
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

    fn shortcuts(&self) -> Vec<(&'static str, &str)> {
        let editor = &self.windows.editor_shortcuts;
        let diary_list = &self.windows.diary_list_shortcuts;
        vec![
            ("bold", &editor.bold),
            ("underline", &editor.underline),
            ("strike", &editor.strike),
            ("heading1", &editor.heading1),
            ("heading2", &editor.heading2),
            ("heading3", &editor.heading3),
            ("heading4", &editor.heading4),
            ("heading5", &editor.heading5),
            ("bulletList", &editor.bullet_list),
            ("taskList", &editor.task_list),
            ("summary", &editor.summary),
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
    let mut value: Value = serde_json::from_slice(bytes)?;
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .ok_or_else(|| SyncedSettingsError::InvalidData("缺少有效的 version 字段".into()))?;
    if version == 0 || version > CURRENT_SYNCED_SETTINGS_VERSION {
        return Err(SyncedSettingsError::UnsupportedVersion {
            found: version,
            supported: CURRENT_SYNCED_SETTINGS_VERSION,
        });
    }

    if version == 1 {
        migrate_v1_to_v2(&mut value)?;
    }
    if value.get("version").and_then(Value::as_u64) == Some(2) {
        migrate_v2_to_v3(&mut value)?;
    }
    if value.get("version").and_then(Value::as_u64) == Some(3) {
        migrate_v3_to_v4(&mut value)?;
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

fn migrate_v1_to_v2(value: &mut Value) -> Result<(), SyncedSettingsError> {
    let toolbar_order = value
        .pointer_mut("/editor/toolbarOrder")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            SyncedSettingsError::InvalidData("缺少有效的 editor.toolbarOrder 字段".into())
        })?;
    toolbar_order.push(Value::String("undo".into()));
    toolbar_order.push(Value::String("redo".into()));
    value["version"] = Value::from(2);
    Ok(())
}

fn migrate_v2_to_v3(value: &mut Value) -> Result<(), SyncedSettingsError> {
    let toolbar_order = value
        .pointer_mut("/editor/toolbarOrder")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            SyncedSettingsError::InvalidData("缺少有效的 editor.toolbarOrder 字段".into())
        })?;
    let heading3_index = toolbar_order
        .iter()
        .position(|action| action.as_str() == Some("heading3"))
        .ok_or_else(|| SyncedSettingsError::InvalidData("编辑器工具栏顺序缺少 heading3".into()))?;
    toolbar_order.insert(heading3_index + 1, Value::String("heading4".into()));
    toolbar_order.insert(heading3_index + 2, Value::String("heading5".into()));
    value["version"] = Value::from(3);
    Ok(())
}

fn migrate_v3_to_v4(value: &mut Value) -> Result<(), SyncedSettingsError> {
    let toolbar_order = value
        .pointer_mut("/editor/toolbarOrder")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            SyncedSettingsError::InvalidData("缺少有效的 editor.toolbarOrder 字段".into())
        })?;
    let task_list_index = toolbar_order
        .iter()
        .position(|action| action.as_str() == Some("taskList"))
        .ok_or_else(|| SyncedSettingsError::InvalidData("编辑器工具栏顺序缺少 taskList".into()))?;
    toolbar_order.insert(task_list_index, Value::String("bulletList".into()));
    // 新增快捷键由 EditorShortcutSettings 的 serde default 填充，保留所有原有自定义值。
    value["version"] = Value::from(4);
    Ok(())
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
                editor_shortcuts: EditorShortcutSettings::default(),
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

        assert_eq!(value["version"], 4);
        assert_eq!(value["updatedAt"], 123);
        assert_eq!(value["appearance"]["theme"], "dark");
        assert_eq!(value["editor"]["toolbarOrder"][6], "heading4");
        assert_eq!(value["editor"]["toolbarOrder"][8], "bulletList");
        assert_eq!(value["editor"]["toolbarOrder"][9], "taskList");
        assert_eq!(value["editor"]["toolbarOrder"][11], "undo");
        assert_eq!(value["editor"]["toolbarOrder"][12], "redo");
        assert_eq!(
            value["windows"]["editorShortcuts"]["bulletList"],
            "Ctrl+Shift+Digit8"
        );
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
        value["version"] = (CURRENT_SYNCED_SETTINGS_VERSION + 1).into();
        assert!(matches!(
            deserialize_synced_settings(&serde_json::to_vec(&value).unwrap()),
            Err(SyncedSettingsError::UnsupportedVersion {
                found: 5,
                supported: 4
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

    #[test]
    fn migrates_v1_toolbar_order_by_appending_undo_and_redo() {
        let mut value = serde_json::to_value(SyncedSettingsDocument {
            version: CURRENT_SYNCED_SETTINGS_VERSION,
            updated_at: 123,
            data: sample_settings(),
        })
        .unwrap();
        value["version"] = 1.into();
        value["editor"]["toolbarOrder"] = serde_json::json!([
            "bold",
            "underline",
            "strike",
            "heading1",
            "heading2",
            "heading3",
            "taskList",
            "summary"
        ]);

        let document = deserialize_synced_settings(&serde_json::to_vec(&value).unwrap()).unwrap();

        assert_eq!(document.version, CURRENT_SYNCED_SETTINGS_VERSION);
        assert_eq!(
            document.data.editor.toolbar_order,
            EditorToolbarAction::ALL.to_vec()
        );
    }

    #[test]
    fn migrates_v2_toolbar_order_by_inserting_new_headings_after_heading3() {
        let mut value = serde_json::to_value(SyncedSettingsDocument {
            version: CURRENT_SYNCED_SETTINGS_VERSION,
            updated_at: 123,
            data: sample_settings(),
        })
        .unwrap();
        value["version"] = 2.into();
        value["editor"]["toolbarOrder"] = serde_json::json!([
            "redo",
            "heading2",
            "heading3",
            "bold",
            "taskList",
            "heading1",
            "underline",
            "summary",
            "strike",
            "undo"
        ]);
        let shortcuts = value["windows"]["editorShortcuts"].as_object_mut().unwrap();
        shortcuts.remove("heading4");
        shortcuts.remove("heading5");

        let document = deserialize_synced_settings(&serde_json::to_vec(&value).unwrap()).unwrap();

        assert_eq!(document.version, CURRENT_SYNCED_SETTINGS_VERSION);
        assert_eq!(
            document.data.editor.toolbar_order,
            vec![
                EditorToolbarAction::Redo,
                EditorToolbarAction::Heading2,
                EditorToolbarAction::Heading3,
                EditorToolbarAction::Heading4,
                EditorToolbarAction::Heading5,
                EditorToolbarAction::Bold,
                EditorToolbarAction::BulletList,
                EditorToolbarAction::TaskList,
                EditorToolbarAction::Heading1,
                EditorToolbarAction::Underline,
                EditorToolbarAction::Summary,
                EditorToolbarAction::Strike,
                EditorToolbarAction::Undo,
            ]
        );
        assert_eq!(
            document.data.windows.editor_shortcuts.heading4,
            "Ctrl+Digit4"
        );
        assert_eq!(
            document.data.windows.editor_shortcuts.heading5,
            "Ctrl+Digit5"
        );
    }

    #[test]
    fn fills_missing_toolbar_shortcuts_with_defaults() {
        let mut value = serde_json::to_value(SyncedSettingsDocument {
            version: CURRENT_SYNCED_SETTINGS_VERSION,
            updated_at: 123,
            data: sample_settings(),
        })
        .unwrap();
        let shortcuts = value["windows"]["editorShortcuts"].as_object_mut().unwrap();
        for field in [
            "bold",
            "underline",
            "strike",
            "heading1",
            "heading2",
            "heading3",
            "heading4",
            "heading5",
            "bulletList",
            "taskList",
            "summary",
        ] {
            shortcuts.remove(field);
        }

        let document = deserialize_synced_settings(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            document.data.windows.editor_shortcuts,
            EditorShortcutSettings::default()
        );
    }

    fn legacy_v3_value() -> Value {
        let mut value = serde_json::to_value(SyncedSettingsDocument {
            version: 3,
            updated_at: 123,
            data: sample_settings(),
        })
        .unwrap();
        value["editor"]["toolbarOrder"]
            .as_array_mut()
            .unwrap()
            .retain(|action| action.as_str() != Some("bulletList"));
        value["windows"]["editorShortcuts"]
            .as_object_mut()
            .unwrap()
            .remove("bulletList");
        value
    }

    #[test]
    fn migrates_v3_list_action_without_changing_existing_order_or_shortcuts() {
        let mut value = legacy_v3_value();
        let order = value["editor"]["toolbarOrder"].as_array_mut().unwrap();
        order.reverse();
        let old_order = order.clone();
        value["windows"]["editorShortcuts"]["bold"] = "Ctrl+Alt+KeyB".into();
        value["windows"]["editorShortcuts"]["taskList"] = "".into();
        let document = deserialize_synced_settings(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(document.version, 4);
        assert_eq!(document.updated_at, 123);
        let upgraded = serde_json::to_value(&document).unwrap();
        let order = upgraded["editor"]["toolbarOrder"].as_array().unwrap();
        let index = order
            .iter()
            .position(|action| action == "bulletList")
            .unwrap();
        assert_eq!(order[index + 1], "taskList");
        assert_eq!(
            order
                .iter()
                .filter(|action| *action != "bulletList")
                .cloned()
                .collect::<Vec<_>>(),
            old_order
        );
        assert_eq!(
            document.data.windows.editor_shortcuts.bullet_list,
            "Ctrl+Shift+Digit8"
        );
        assert_eq!(document.data.windows.editor_shortcuts.bold, "Ctrl+Alt+KeyB");
        assert_eq!(document.data.windows.editor_shortcuts.task_list, "");
        // 重读最新格式不应再次插入新按钮，也不能覆盖用户移动后的顺序。
        let mut moved = document;
        moved
            .data
            .editor
            .toolbar_order
            .retain(|action| *action != EditorToolbarAction::BulletList);
        moved
            .data
            .editor
            .toolbar_order
            .insert(0, EditorToolbarAction::BulletList);
        assert_eq!(
            deserialize_synced_settings(&serde_json::to_vec(&moved).unwrap()).unwrap(),
            moved
        );
    }

    #[test]
    fn rejects_incomplete_or_duplicate_v3_toolbar_orders() {
        for missing in ["taskList", "bold"] {
            let mut value = legacy_v3_value();
            value["editor"]["toolbarOrder"]
                .as_array_mut()
                .unwrap()
                .retain(|action| action != missing);
            assert!(matches!(
                deserialize_synced_settings(&serde_json::to_vec(&value).unwrap()),
                Err(SyncedSettingsError::InvalidData(_))
            ));
        }
        let mut value = legacy_v3_value();
        value["editor"]["toolbarOrder"]
            .as_array_mut()
            .unwrap()
            .push("taskList".into());
        assert!(matches!(
            deserialize_synced_settings(&serde_json::to_vec(&value).unwrap()),
            Err(SyncedSettingsError::InvalidData(_))
        ));
    }

    #[test]
    fn validates_list_shortcut_length_and_preserves_it_in_latest_documents() {
        let mut data = sample_settings();
        data.windows.editor_shortcuts.bullet_list = "L".repeat(MAX_SHORTCUT_CHARS + 1);
        assert!(matches!(
            data.validate(),
            Err(SyncedSettingsError::InvalidData(_))
        ));
        data.windows.editor_shortcuts.bullet_list = "Ctrl+Alt+KeyL".into();
        let document = SyncedSettingsDocument {
            version: 4,
            updated_at: 123,
            data,
        };
        assert_eq!(
            deserialize_synced_settings(&serde_json::to_vec(&document).unwrap()).unwrap(),
            document
        );
    }
}
