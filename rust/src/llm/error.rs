use thiserror::Error;

/// LLM-specific errors
#[derive(Error, Debug)]
pub enum LlmError {
    #[error("API request failed with status {status}: {message}")]
    ApiError { status: u16, message: String },

    #[error("Failed to build HTTP client: {0}")]
    ClientBuildFailed(String),

    #[error("Request timeout")]
    Timeout,

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Invalid API key")]
    InvalidApiKey,

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, LlmError>;
