use crate::core::{MantraError, Result};
use reqwest::{header, Client};

use super::types::{CompletionRequest, CompletionResponse};
use crate::config::Config;

/// LLM API client
#[derive(Clone)]
pub struct LLMClient {
    client: Client,
    config: Config,
}

impl LLMClient {
    /// Create a new LLM client
    pub fn new(config: Config) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        // Add API key if provided
        if let Some(api_key) = &config.api_key {
            headers.insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&format!("Bearer {}", api_key))
                    .map_err(|e| MantraError::config(format!("Invalid API key format: {}", e)))?,
            );
        }

        // Add app identification headers (primarily for OpenRouter, but safe for all providers)
        // These headers help with app discovery on platforms that support them
        headers.insert(
            "HTTP-Referer",
            header::HeaderValue::from_static("https://github.com/rail44/mantra"),
        );
        headers.insert("X-Title", header::HeaderValue::from_static("mantra"));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| MantraError::llm(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    /// Get the model name
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Get OpenRouter configuration if available
    pub fn openrouter_config(&self) -> Option<&crate::config::OpenRouterConfig> {
        self.config.openrouter.as_ref()
    }

    /// Send a completion request
    pub async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let url = format!("{}/chat/completions", self.config.url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| MantraError::llm(format!("Failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(MantraError::llm(format!(
                "API request failed with status {}: {}",
                status, error_text
            )));
        }

        let completion = response
            .json::<CompletionResponse>()
            .await
            .map_err(|e| MantraError::llm(format!("Failed to parse response: {}", e)))?;

        Ok(completion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OpenRouterConfig;
    use crate::llm::Message;

    #[test]
    fn test_client_creation() {
        let config = Config {
            model: "gpt-3.5-turbo".to_string(),
            url: "https://api.openai.com".to_string(),
            api_key: Some("test-key".to_string()),
            log_level: None,
            openrouter: None,
        };

        let client = LLMClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_with_openrouter() {
        let config = Config {
            model: "openai/gpt-3.5-turbo".to_string(),
            url: "https://openrouter.ai".to_string(),
            api_key: Some("test-key".to_string()),
            log_level: None,
            openrouter: Some(OpenRouterConfig {
                providers: vec!["openai".to_string(), "anthropic".to_string()],
            }),
        };

        let client = LLMClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_without_api_key() {
        let config = Config {
            model: "local-model".to_string(),
            url: "http://localhost:8080".to_string(),
            api_key: None,
            log_level: None,
            openrouter: None,
        };

        let client = LLMClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_invalid_api_key() {
        let config = Config {
            model: "test".to_string(),
            url: "https://api.test.com".to_string(),
            api_key: Some("\0invalid\0key".to_string()), // Invalid header value
            log_level: None,
            openrouter: None,
        };

        let client = LLMClient::new(config);
        assert!(client.is_err());
        if let Err(err) = client {
            assert!(err.to_string().contains("Invalid API key format"));
        }
    }

    #[test]
    fn test_model_getter() {
        let config = Config {
            model: "claude-3-sonnet".to_string(),
            url: "https://api.anthropic.com".to_string(),
            api_key: Some("test-key".to_string()),
            log_level: None,
            openrouter: None,
        };

        let client = LLMClient::new(config).unwrap();
        assert_eq!(client.model(), "claude-3-sonnet");
    }

    #[test]
    fn test_request_building() {
        // This test would require mocking the HTTP client
        // For now, we just test that request building doesn't panic
        let messages = vec![
            Message::system("You are a helpful assistant"),
            Message::user("Write a function that adds two numbers"),
        ];

        let request = CompletionRequest {
            model: "gpt-3.5-turbo".to_string(),
            provider: None,
            messages,
            max_tokens: Some(150),
            temperature: 0.7,
        };

        // Verify fields are set correctly
        assert_eq!(request.model, "gpt-3.5-turbo");
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.max_tokens, Some(150));
        assert_eq!(request.temperature, 0.7);
    }
}
