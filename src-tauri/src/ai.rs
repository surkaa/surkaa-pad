mod agent;
#[cfg(test)]
mod agent_tests;
pub mod ai_command;
mod client;
#[cfg(test)]
mod client_tests;
mod config;
mod diary_tools;
mod error;
mod openai_protocol;
mod provider;
mod tools;
mod types;

pub(crate) use agent::AiAgentRunResult;
pub use agent::{AiAgent, AiAgentEvent, AiAgentResponse};
pub use client::OpenAiCompatibleClient;
pub use config::AiProviderConfig;
pub use diary_tools::DiaryReadTools;
pub use error::AiError;
pub use provider::AiModelProvider;
pub use tools::{AiToolCallDisplay, AiToolError, AiToolExecutor};
pub use types::{
    AiAssistantMessage, AiCompletion, AiCompletionDelta, AiCompletionRequest, AiConversationSource,
    AiConversationSourceMessage, AiConversationSourceToolCall, AiConversationTurn, AiMessage,
    AiModel, AiToolCall, AiToolDefinition, AiToolResult, AiUsage,
};
