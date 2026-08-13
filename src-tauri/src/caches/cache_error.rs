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

    #[error("Invalid empty etag")]
    InvalidEtag,

    #[error("Path error: {0}")]
    PathError(String),

    #[error("Cache metadata error: {0}")]
    Metadata(String),

    #[error("Attachment size {required_bytes} exceeds local cache limit {limit_bytes}")]
    CapacityExceeded {
        required_bytes: u64,
        limit_bytes: u64,
    },

    #[error(
        "Unable to free enough local cache space for {required_bytes} bytes within limit {limit_bytes}"
    )]
    InsufficientEvictableCapacity {
        required_bytes: u64,
        limit_bytes: u64,
    },
}

impl From<CacheError> for crate::error::AppError {
    fn from(e: CacheError) -> Self {
        crate::error::AppError {
            error_type: "cache".into(),
            message: e.to_string(),
        }
    }
}
