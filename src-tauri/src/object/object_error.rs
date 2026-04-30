use thiserror::Error;

#[derive(Debug, Error)]
pub enum ObjectError {
    #[error("Failed to create s3 client: {0}")]
    CreateFailed(String),

    #[error("Key already exists: {0}")]
    KeyAlreadyExists(String),

    #[error("Object storage operation failed: {0}")]
    OperationFailed(String),

    #[error("OSS client not initialized")]
    NotInitialized,
}

impl From<ObjectError> for crate::error::AppError {
    fn from(e: ObjectError) -> Self {
        crate::error::AppError {
            error_type: "object".into(),
            message: e.to_string(),
        }
    }
}
