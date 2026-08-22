use super::types::{
    deserialize_synced_settings, SyncedSettingsData, SyncedSettingsDocument, SyncedSettingsError,
    CURRENT_SYNCED_SETTINGS_VERSION,
};
use crate::app_object_store::{AppObjectStoreError, SharedAppObjectStore};
use crate::cryptos::{Crypto, CryptoError};
use crate::error::AppError;
use crate::object_locations::StoredObject;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncedSettingsRepositoryError {
    #[error("同步设置对象存储失败: {0}")]
    Store(#[from] AppObjectStoreError),
    #[error("同步设置加解密失败: {0}")]
    Crypto(#[from] CryptoError),
    #[error("同步设置数据无效: {0}")]
    Data(#[from] SyncedSettingsError),
    #[error("同步设置 JSON 序列化失败: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl From<SyncedSettingsRepositoryError> for AppError {
    fn from(error: SyncedSettingsRepositoryError) -> Self {
        Self {
            error_type: "synced_settings".into(),
            message: error.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct SyncedSettingsRepository {
    store: SharedAppObjectStore,
    crypto: Crypto,
}

impl SyncedSettingsRepository {
    pub fn new(store: SharedAppObjectStore, crypto: Crypto) -> Self {
        Self { store, crypto }
    }

    pub async fn load(
        &self,
    ) -> Result<Option<SyncedSettingsDocument>, SyncedSettingsRepositoryError> {
        let Some(encrypted) = self.store.load_bytes(&StoredObject::SyncedSettings).await? else {
            return Ok(None);
        };
        let plaintext = self.crypto.decrypt(&encrypted)?;
        Ok(Some(deserialize_synced_settings(&plaintext)?))
    }

    pub async fn save(
        &self,
        data: SyncedSettingsData,
        updated_at: i64,
    ) -> Result<SyncedSettingsDocument, SyncedSettingsRepositoryError> {
        data.validate()?;
        if updated_at < 0 {
            return Err(SyncedSettingsError::InvalidData("更新时间不能为负数".into()).into());
        }
        let document = SyncedSettingsDocument {
            version: CURRENT_SYNCED_SETTINGS_VERSION,
            updated_at,
            data,
        };
        let plaintext = serde_json::to_vec(&document)?;
        let encrypted = self.crypto.encrypt(&plaintext)?;
        self.store
            .save_bytes(&StoredObject::SyncedSettings, &encrypted)
            .await?;
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_object_store::{AppObjectStore, LocalAppObjectStore};
    use crate::caches::LocalObjectStore;
    use crate::object_locations::ObjectLocations;
    use crate::synced_settings::types::{
        AiAssistantShortcutSettings, AttachmentSettings, DiaryListShortcutSettings, EditorSettings,
        EditorShortcutSettings, EditorToolbarAction, SyncedAppearanceSettings, SyncedTheme,
        WindowsSettings,
    };
    use std::sync::Arc;

    fn sample_settings() -> SyncedSettingsData {
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
                toolbar_order: vec![
                    EditorToolbarAction::Bold,
                    EditorToolbarAction::Underline,
                    EditorToolbarAction::Strike,
                    EditorToolbarAction::Heading1,
                    EditorToolbarAction::Heading2,
                    EditorToolbarAction::Heading3,
                    EditorToolbarAction::TaskList,
                    EditorToolbarAction::Summary,
                ],
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

    fn test_crypto() -> Crypto {
        let crypto = Crypto::new();
        crypto
            .derive_dek(
                "synced-settings-password".into(),
                "c3luY2VkLXNldHRpbmdzLXRlc3Qtc2FsdA",
            )
            .unwrap();
        crypto
    }

    #[tokio::test]
    async fn roundtrips_encrypted_settings_and_handles_missing_object() {
        let temp = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp.path().to_path_buf());
        let store = Arc::new(LocalAppObjectStore::new(local.clone()));
        let repository = SyncedSettingsRepository::new(store.clone(), test_crypto());

        assert_eq!(repository.load().await.unwrap(), None);
        let saved = repository.save(sample_settings(), 123).await.unwrap();
        assert_eq!(repository.load().await.unwrap(), Some(saved.clone()));

        let encrypted = local
            .get_data(ObjectLocations::synced_settings())
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&encrypted).contains("Ctrl+Alt+KeyP"));
        assert_eq!(saved.version, CURRENT_SYNCED_SETTINGS_VERSION);
        assert_eq!(saved.updated_at, 123);

        store.delete(&StoredObject::SyncedSettings).await.unwrap();
        assert_eq!(repository.load().await.unwrap(), None);
    }

    #[tokio::test]
    async fn rejects_tampered_or_unsupported_documents() {
        let temp = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp.path().to_path_buf());
        let store = Arc::new(LocalAppObjectStore::new(local));
        let crypto = test_crypto();
        let repository = SyncedSettingsRepository::new(store.clone(), crypto.clone());

        let mut value = serde_json::to_value(SyncedSettingsDocument {
            version: CURRENT_SYNCED_SETTINGS_VERSION,
            updated_at: 123,
            data: sample_settings(),
        })
        .unwrap();
        value["version"] = 2.into();
        let encrypted = crypto
            .encrypt(&serde_json::to_vec(&value).unwrap())
            .unwrap();
        store
            .save_bytes(&StoredObject::SyncedSettings, &encrypted)
            .await
            .unwrap();
        assert!(matches!(
            repository.load().await,
            Err(SyncedSettingsRepositoryError::Data(
                SyncedSettingsError::UnsupportedVersion { .. }
            ))
        ));

        store
            .save_bytes(&StoredObject::SyncedSettings, b"not-valid-ciphertext")
            .await
            .unwrap();
        assert!(matches!(
            repository.load().await,
            Err(SyncedSettingsRepositoryError::Crypto(_))
        ));
    }
}
