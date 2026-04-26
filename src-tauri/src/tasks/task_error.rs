use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("Task pool lock poisoned: {0}")]
    LockPoisoned(String),
}

impl From<TaskError> for crate::error::AppError {
    fn from(e: TaskError) -> Self {
        crate::error::AppError {
            error_type: "task".into(),
            message: e.to_string(),
        }
    }
}
