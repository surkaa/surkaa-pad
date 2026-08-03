use super::{AiCompletion, AiCompletionDelta, AiCompletionRequest, AiError, AiModel};
use async_trait::async_trait;

#[async_trait]
pub trait AiModelProvider: Send + Sync {
    async fn list_models(&self) -> Result<Vec<AiModel>, AiError>;

    async fn complete(&self, request: AiCompletionRequest) -> Result<AiCompletion, AiError>;

    async fn complete_stream(
        &self,
        request: AiCompletionRequest,
        on_delta: &(dyn Fn(AiCompletionDelta) -> Result<(), AiError> + Send + Sync),
    ) -> Result<AiCompletion, AiError> {
        let completion = self.complete(request).await?;
        if let Some(reasoning) = completion.message.reasoning_content.clone() {
            on_delta(AiCompletionDelta::Reasoning(reasoning))?;
        }
        if let Some(content) = completion.message.content.clone() {
            on_delta(AiCompletionDelta::Content(content))?;
        }
        Ok(completion)
    }
}
