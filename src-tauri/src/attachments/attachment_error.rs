use thiserror::Error;

#[derive(Debug, Error)]
pub enum AttachmentError {
    #[error("ID assignment failed")]
    IdAssignmentFailed,

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Image processing failed: {0}")]
    ImageProcessingFailed(String),

    #[error("File operation failed: {0}")]
    FileOperationFailed(String),

    #[error("Object storage error: {0}")]
    Object(#[from] crate::object::ObjectError),

    #[error("Local cache error: {0}")]
    Cache(#[from] crate::caches::CacheError),

    #[error("Crypto error: {0}")]
    Crypto(#[from] crate::cryptos::CryptoError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Diary error: {0}")]
    Diary(#[from] crate::diaries::DiaryError),
}

impl From<AttachmentError> for crate::error::AppError {
    fn from(e: AttachmentError) -> Self {
        crate::error::AppError {
            error_type: "attachment".into(),
            message: e.to_string(),
        }
    }
}
