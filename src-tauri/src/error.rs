use serde::Serialize;
use specta::Type;

#[derive(Debug, Serialize, Type)]
pub struct AppError {
    pub error_type: String,
    pub message: String,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.error_type, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError {
            error_type: "io".into(),
            message: e.to_string(),
        }
    }
}
