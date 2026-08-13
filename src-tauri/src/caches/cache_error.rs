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
        "附件大小 {attachment_display} 超过单个附件缓存上限 {limit_display}，请在设置中调高后重试",
        attachment_display = display_size(*attachment_bytes),
        limit_display = display_size(*limit_bytes)
    )]
    AttachmentTooLarge {
        attachment_bytes: u64,
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

fn display_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.2} GB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MB", bytes as f64 / MIB as f64)
    } else {
        format!("{bytes} B")
    }
}

impl From<CacheError> for crate::error::AppError {
    fn from(e: CacheError) -> Self {
        crate::error::AppError {
            error_type: "cache".into(),
            message: e.to_string(),
        }
    }
}
