use super::AiError;
use reqwest::Url;
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct AiProviderConfig {
    base_url: Url,
    api_key: Option<String>,
}

impl AiProviderConfig {
    pub fn new(base_url: &str, api_key: Option<String>) -> Result<Self, AiError> {
        let mut base_url = Url::parse(base_url.trim())
            .map_err(|error| AiError::InvalidBaseUrl(error.to_string()))?;
        validate_url_shape(&base_url)?;

        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }

        Ok(Self {
            base_url,
            api_key: api_key.and_then(normalize_api_key),
        })
    }

    pub fn base_url(&self) -> &str {
        self.base_url.as_str()
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub fn models_url(&self) -> Url {
        self.base_url
            .join("models")
            .expect("validated AI base URL must accept relative paths")
    }

    pub fn chat_completions_url(&self) -> Url {
        self.base_url
            .join("chat/completions")
            .expect("validated AI base URL must accept relative paths")
    }
}

impl fmt::Debug for AiProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AiProviderConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

fn validate_url_shape(url: &Url) -> Result<(), AiError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AiError::InvalidBaseUrl("仅支持 http 或 https 协议".into()));
    }
    if url.host_str().is_none() {
        return Err(AiError::InvalidBaseUrl("缺少主机名".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AiError::InvalidBaseUrl(
            "请通过 API Key 字段提供凭证，不要把凭证写入地址".into(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(AiError::InvalidBaseUrl(
            "服务地址不能包含查询参数或片段".into(),
        ));
    }
    Ok(())
}

fn normalize_api_key(api_key: String) -> Option<String> {
    let api_key = api_key.trim();
    (!api_key.is_empty()).then(|| api_key.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_url_is_normalized_and_builds_openai_endpoints() {
        let config = AiProviderConfig::new(" http://localhost:11434/v1 ", None).unwrap();

        assert_eq!(config.base_url(), "http://localhost:11434/v1/");
        assert_eq!(
            config.models_url().as_str(),
            "http://localhost:11434/v1/models"
        );
        assert_eq!(
            config.chat_completions_url().as_str(),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn remote_http_endpoint_is_accepted() {
        let config =
            AiProviderConfig::new("http://192.168.1.10:11434/v1/", Some(" secret ".into()))
                .unwrap();

        assert_eq!(config.api_key(), Some("secret"));
        assert_eq!(
            config.models_url().as_str(),
            "http://192.168.1.10:11434/v1/models"
        );
    }

    #[test]
    fn unsupported_protocol_and_ambiguous_urls_are_rejected() {
        for url in [
            "ftp://localhost/v1",
            "https://user:password@example.com/v1",
            "https://example.com/v1?token=secret",
            "https://example.com/v1#models",
        ] {
            assert!(matches!(
                AiProviderConfig::new(url, None),
                Err(AiError::InvalidBaseUrl(_))
            ));
        }
    }

    #[test]
    fn blank_api_key_is_treated_as_missing() {
        let config = AiProviderConfig::new("http://localhost:11434/v1", Some("  ".into())).unwrap();

        assert_eq!(config.api_key(), None);
    }

    #[test]
    fn debug_output_redacts_api_key() {
        let config =
            AiProviderConfig::new("https://example.com/v1", Some("super-secret-key".into()))
                .unwrap();
        let output = format!("{config:?}");

        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("super-secret-key"));
    }
}
