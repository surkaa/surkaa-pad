use thiserror::Error;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Cache entry not found")]
    NotFound,

    #[error("Cache already finalized")]
    AlreadyFinalized,

    #[error("Stream error occurred during caching")]
    StreamError,

    #[error("File or context missing")]
    FileOrContextMissing,

    #[error("Invalid filename")]
    InvalidFilename,

    #[error("Path error: {0}")]
    PathError(String),
}

impl From<CacheError> for crate::error::AppError {
    fn from(e: CacheError) -> Self {
        crate::error::AppError {
            error_type: "cache".into(),
            message: e.to_string(),
        }
    }
}
