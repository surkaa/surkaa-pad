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
mod message_blocks;
mod openai_protocol;
mod provider;
mod session_repository;
mod session_types;
mod tools;
mod types;

pub(crate) use agent::AiAgentRunResult;
pub use agent::{AiAgent, AiAgentEvent, AiAgentResponse};
pub use client::OpenAiCompatibleClient;
pub use config::AiProviderConfig;
pub use diary_tools::DiaryReadTools;
pub use error::AiError;
pub use message_blocks::{
    append_and_compact_message, load_all_compacted_messages, load_compacted_messages,
    AiMessageBlockError, AiMessageBlockStore,
};
pub use provider::AiModelProvider;
pub use session_repository::{AiSessionRepository, AiSessionRepositoryError};
pub use session_types::{
    ai_message_block_size, deserialize_session_message_block, deserialize_session_meta,
    migrate_session_document, AiAssistantRecordState, AiProcessStepKind, AiProcessStepRecord,
    AiProcessStepState, AiSessionDataError, AiSessionMessage, AiSessionMessageBlock,
    AiSessionMessagePayload, AiSessionMeta, CURRENT_AI_SESSION_VERSION,
};
pub use tools::{AiToolCallDisplay, AiToolError, AiToolExecutor};
pub use types::{
    AiAssistantMessage, AiCompletion, AiCompletionDelta, AiCompletionRequest, AiConversationSource,
    AiConversationSourceMessage, AiConversationSourceToolCall, AiConversationTurn, AiMessage,
    AiModel, AiToolCall, AiToolDefinition, AiToolResult, AiUsage,
};
