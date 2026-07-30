use super::{AiCompletion, AiCompletionRequest, AiError, AiModel};
use async_trait::async_trait;

#[async_trait]
pub trait AiModelProvider: Send + Sync {
    async fn list_models(&self) -> Result<Vec<AiModel>, AiError>;

    async fn complete(&self, request: AiCompletionRequest) -> Result<AiCompletion, AiError>;
}
