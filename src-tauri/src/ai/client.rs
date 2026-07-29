use super::{AiError, AiModel, AiProviderConfig};
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder, Response};
use serde::Deserialize;
use std::time::Duration;

const MODELS_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_MODELS_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
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

    pub async fn list_models(&self) -> Result<Vec<AiModel>, AiError> {
        let request = self.authorize(self.http.get(self.config.models_url()));
        let response = request
            .timeout(MODELS_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| AiError::RequestFailed(error.to_string()))?;
        let status = response.status();
        let body = read_limited_body(response, MAX_MODELS_RESPONSE_BYTES).await?;

        if !status.is_success() {
            return Err(AiError::HttpStatus {
                status: status.as_u16(),
                message: response_error_message(&body),
            });
        }

        let response: ModelListResponse = serde_json::from_slice(&body)
            .map_err(|error| AiError::InvalidResponse(error.to_string()))?;
        if response.data.iter().any(|model| model.id.trim().is_empty()) {
            return Err(AiError::InvalidResponse("模型列表中存在空的模型 ID".into()));
        }

        Ok(response
            .data
            .into_iter()
            .map(|model| AiModel {
                id: model.id,
                owned_by: model.owned_by,
            })
            .collect())
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        match self.config.api_key() {
            Some(api_key) => request.bearer_auth(api_key),
            None => request,
        }
    }
}

#[derive(Deserialize)]
struct ModelListResponse {
    data: Vec<ModelObject>,
}

#[derive(Deserialize)]
struct ModelObject {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn lists_models_and_sends_optional_bearer_token() {
        let (base_url, request) = spawn_json_response(
            200,
            r#"{"data":[{"id":"qwen3:8b","owned_by":"ollama"},{"id":"local-model"}]}"#,
        )
        .await;
        let config = AiProviderConfig::new(&base_url, Some("test-secret".into())).unwrap();
        let client = OpenAiCompatibleClient::new(config).unwrap();

        let models = client.list_models().await.unwrap();

        assert_eq!(
            models,
            vec![
                AiModel {
                    id: "qwen3:8b".into(),
                    owned_by: Some("ollama".into()),
                },
                AiModel {
                    id: "local-model".into(),
                    owned_by: None,
                },
            ]
        );
        let request = request.await.unwrap();
        assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-secret\r\n"));
    }

    #[tokio::test]
    async fn omits_authorization_header_when_api_key_is_missing() {
        let (base_url, request) = spawn_json_response(200, r#"{"data":[]}"#).await;
        let config = AiProviderConfig::new(&base_url, None).unwrap();
        let client = OpenAiCompatibleClient::new(config).unwrap();

        assert!(client.list_models().await.unwrap().is_empty());
        assert!(!request
            .await
            .unwrap()
            .to_ascii_lowercase()
            .contains("authorization:"));
    }

    #[tokio::test]
    async fn maps_openai_error_response_without_exposing_the_api_key() {
        let (base_url, _) =
            spawn_json_response(401, r#"{"error":{"message":"API Key 无效"}}"#).await;
        let config = AiProviderConfig::new(&base_url, Some("secret-not-in-error".into())).unwrap();
        let client = OpenAiCompatibleClient::new(config).unwrap();

        let error = client.list_models().await.unwrap_err();

        assert_eq!(
            error,
            AiError::HttpStatus {
                status: 401,
                message: "API Key 无效".into(),
            }
        );
        assert!(!error.to_string().contains("secret-not-in-error"));
    }

    #[tokio::test]
    async fn rejects_invalid_model_list_response() {
        let (base_url, _) = spawn_json_response(200, r#"{"models":[]}"#).await;
        let config = AiProviderConfig::new(&base_url, None).unwrap();
        let client = OpenAiCompatibleClient::new(config).unwrap();

        assert!(matches!(
            client.list_models().await,
            Err(AiError::InvalidResponse(_))
        ));
    }

    #[tokio::test]
    async fn rejects_empty_model_ids() {
        let (base_url, _) = spawn_json_response(200, r#"{"data":[{"id":"  "}]}"#).await;
        let config = AiProviderConfig::new(&base_url, None).unwrap();
        let client = OpenAiCompatibleClient::new(config).unwrap();

        assert_eq!(
            client.list_models().await,
            Err(AiError::InvalidResponse("模型列表中存在空的模型 ID".into()))
        );
    }

    #[tokio::test]
    async fn rejects_oversized_model_list_before_reading_the_body() {
        let (base_url, _) =
            spawn_response(200, "{}", Some(MAX_MODELS_RESPONSE_BYTES.saturating_add(1))).await;
        let config = AiProviderConfig::new(&base_url, None).unwrap();
        let client = OpenAiCompatibleClient::new(config).unwrap();

        assert_eq!(
            client.list_models().await,
            Err(AiError::ResponseTooLarge {
                limit_bytes: MAX_MODELS_RESPONSE_BYTES,
            })
        );
    }

    async fn spawn_json_response(
        status: u16,
        body: &'static str,
    ) -> (String, oneshot::Receiver<String>) {
        spawn_response(status, body, None).await
    }

    async fn spawn_response(
        status: u16,
        body: &'static str,
        declared_content_length: Option<usize>,
    ) -> (String, oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (request_sender, request_receiver) = oneshot::channel();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            let _ = request_sender.send(String::from_utf8(request).unwrap());

            let reason = match status {
                200 => "OK",
                401 => "Unauthorized",
                _ => "Error",
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                declared_content_length.unwrap_or(body.len())
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        (format!("http://{address}/v1"), request_receiver)
    }
}
