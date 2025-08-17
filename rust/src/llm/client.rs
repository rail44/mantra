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
}
