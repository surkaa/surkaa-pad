use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiaryError {
    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Diary ID is empty")]
    EmptyId,

    #[error("Attachment not found: {0}")]
    AttachmentNotFound(String),

    #[error("Invalid diary manifest: {0}")]
    InvalidManifest(String),

    #[error("Object storage error: {0}")]
    Object(#[from] crate::object::ObjectError),

    #[error("Local cache error: {0}")]
    Cache(#[from] crate::caches::CacheError),

    #[error("Crypto error: {0}")]
    Crypto(#[from] crate::cryptos::CryptoError),
}

impl From<DiaryError> for crate::error::AppError {
    fn from(e: DiaryError) -> Self {
        crate::error::AppError {
            error_type: "diary".into(),
            message: e.to_string(),
        }
    }
}
