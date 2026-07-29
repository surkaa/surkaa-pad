mod client;
mod config;
mod error;
mod types;

pub use client::OpenAiCompatibleClient;
pub use config::AiProviderConfig;
pub use error::AiError;
pub use types::AiModel;
