use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiaryError {
    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Attachment not found: {0}")]
    AttachmentNotFound(String),

    #[error("Invalid diary manifest: {0}")]
    InvalidManifest(String),

    #[error("Attachment upload failed: {0}")]
    AttachmentUpload(String),

    #[error(
        "Diary manifest version {found} is unsupported; this app supports version {supported}"
    )]
    UnsupportedVersion { found: u32, supported: u32 },

    #[error("Object storage error: {0}")]
    Object(#[from] crate::object::ObjectError),

    #[error("Local cache error: {0}")]
    Cache(#[from] crate::caches::CacheError),

    #[error("Crypto error: {0}")]
    Crypto(#[from] crate::cryptos::CryptoError),
}

impl From<DiaryError> for crate::error::AppError {
    fn from(e: DiaryError) -> Self {
        match e {
            DiaryError::UnsupportedVersion { found, supported } if found > supported => {
                crate::error::AppError {
                    error_type: "diary_version_too_new".into(),
                    message: format!(
                        "Diary manifest version {found} is newer than the supported version {supported}"
                    ),
                }
            }
            DiaryError::UnsupportedVersion { found, supported } => crate::error::AppError {
                error_type: "diary_version_too_old".into(),
                message: format!(
                    "日记数据版本 V{found} 过旧，当前应用仅支持 V{supported}"
                ),
            },
            other => crate::error::AppError {
                error_type: "diary".into(),
                message: other.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DiaryError;

    #[test]
    fn unsupported_versions_map_to_stable_directional_error_types() {
        let newer: crate::error::AppError = DiaryError::UnsupportedVersion {
            found: 5,
            supported: 4,
        }
        .into();
        assert_eq!(newer.error_type, "diary_version_too_new");
        assert!(newer.message.contains('5'));

        let legacy: crate::error::AppError = DiaryError::UnsupportedVersion {
            found: 3,
            supported: 4,
        }
        .into();
        assert_eq!(legacy.error_type, "diary_version_too_old");
        assert!(legacy.message.contains("V3"));
        assert!(legacy.message.contains("V4"));
    }
}
