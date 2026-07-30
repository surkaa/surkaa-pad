use super::openai_protocol::{ChatCompletionRequest, ChatCompletionResponse, ModelListResponse};
use super::{
    AiCompletion, AiCompletionRequest, AiError, AiModel, AiModelProvider, AiProviderConfig,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder, Response};
use serde::Deserialize;
use std::time::Duration;

const MODELS_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const MAX_MODELS_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const COMPLETION_REQUEST_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_COMPLETION_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_ERROR_MESSAGE_CHARS: usize = 500;

#[derive(Clone)]
pub struct OpenAiCompatibleClient {
    http: Client,
    config: AiProviderConfig,
}

impl OpenAiCompatibleClient {
    pub fn new(config: AiProviderConfig) -> Result<Self, AiError> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| AiError::RequestFailed(error.to_string()))?;
        Ok(Self { http, config })
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        match self.config.api_key() {
            Some(api_key) => request.bearer_auth(api_key),
            None => request,
        }
    }
}

#[async_trait]
impl AiModelProvider for OpenAiCompatibleClient {
    async fn list_models(&self) -> Result<Vec<AiModel>, AiError> {
        let request = self.authorize(self.http.get(self.config.models_url()));
        let body = send_request(request, MODELS_REQUEST_TIMEOUT, MAX_MODELS_RESPONSE_BYTES).await?;
        let response: ModelListResponse = serde_json::from_slice(&body)
            .map_err(|error| AiError::InvalidResponse(error.to_string()))?;
        response.into_models()
    }

    async fn complete(&self, request: AiCompletionRequest) -> Result<AiCompletion, AiError> {
        let payload = ChatCompletionRequest::from(&request);
        let request = self.authorize(
            self.http
                .post(self.config.chat_completions_url())
                .json(&payload),
        );
        let body = send_request(
            request,
            COMPLETION_REQUEST_TIMEOUT,
            MAX_COMPLETION_RESPONSE_BYTES,
        )
        .await?;
        let response: ChatCompletionResponse = serde_json::from_slice(&body)
            .map_err(|error| AiError::InvalidResponse(error.to_string()))?;
        response.try_into()
    }
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: String,
}

async fn send_request(
    request: RequestBuilder,
    timeout: Duration,
    response_limit: usize,
) -> Result<Vec<u8>, AiError> {
    let response = request
        .timeout(timeout)
        .send()
        .await
        .map_err(|error| AiError::RequestFailed(error.to_string()))?;
    let status = response.status();
    let body = read_limited_body(response, response_limit).await?;

    if !status.is_success() {
        return Err(AiError::HttpStatus {
            status: status.as_u16(),
            message: response_error_message(&body),
        });
    }
    Ok(body)
}

async fn read_limited_body(response: Response, limit: usize) -> Result<Vec<u8>, AiError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(AiError::ResponseTooLarge { limit_bytes: limit });
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| AiError::RequestFailed(error.to_string()))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(AiError::ResponseTooLarge { limit_bytes: limit });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn response_error_message(body: &[u8]) -> String {
    if let Ok(error) = serde_json::from_slice::<ErrorEnvelope>(body) {
        return truncate_message(&error.error.message);
    }

    let plain_text = String::from_utf8_lossy(body);
    let plain_text = plain_text.trim();
    if plain_text.is_empty() {
        "服务未提供错误详情".into()
    } else {
        truncate_message(plain_text)
    }
}

fn truncate_message(message: &str) -> String {
    let mut chars = message.chars();
    let truncated: String = chars.by_ref().take(MAX_ERROR_MESSAGE_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}
