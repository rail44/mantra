use thiserror::Error;

/// LLM-specific errors
#[derive(Error, Debug)]
pub enum LlmError {
    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
