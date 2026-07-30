mod client;
#[cfg(test)]
mod client_tests;
mod config;
mod error;
mod openai_protocol;
mod provider;
mod types;

pub use client::OpenAiCompatibleClient;
pub use config::AiProviderConfig;
pub use error::AiError;
pub use provider::AiModelProvider;
pub use types::{
    AiAssistantMessage, AiCompletion, AiCompletionRequest, AiMessage, AiModel, AiToolCall,
    AiToolDefinition, AiToolResult, AiUsage,
};
