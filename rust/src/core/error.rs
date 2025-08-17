use thiserror::Error;

/// Main application error type that aggregates domain-specific errors
#[derive(Error, Debug)]
pub enum MantraError {
    /// Configuration layer errors
    #[error(transparent)]
    Config(#[from] crate::config::error::ConfigError),

    /// Parser layer errors
    #[error(transparent)]
    Parse(#[from] crate::parser::error::ParseError),

    /// LSP layer errors
    #[error(transparent)]
    Lsp(#[from] crate::lsp::error::LspError),

    /// LLM layer errors
    #[error(transparent)]
    Llm(#[from] crate::llm::error::LlmError),

    /// Actor system errors
    #[error("Actor system error: {0}")]
    Actor(String),

    /// Generic I/O errors not covered by specific layers
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic errors (for migration from anyhow)
    #[error("{0}")]
    Other(String),
}

/// Result type alias for Mantra operations
pub type Result<T> = std::result::Result<T, MantraError>;

// Helper trait for converting anyhow errors during migration
impl From<anyhow::Error> for MantraError {
    fn from(err: anyhow::Error) -> Self {
        MantraError::Other(err.to_string())
    }
}

// Helper methods for creating specific errors
impl MantraError {
    pub fn config(msg: impl Into<String>) -> Self {
        MantraError::Config(crate::config::error::ConfigError::Invalid(msg.into()))
    }

    pub fn parse(_msg: impl Into<String>) -> Self {
        MantraError::Parse(crate::parser::error::ParseError::ParseFailed)
    }

    pub fn lsp(msg: impl Into<String>) -> Self {
        MantraError::Lsp(crate::lsp::error::LspError::InvalidResponse(msg.into()))
    }

    pub fn llm(msg: impl Into<String>) -> Self {
        MantraError::Llm(crate::llm::error::LlmError::InvalidResponse(msg.into()))
    }

    pub fn tree_sitter(_msg: impl Into<String>) -> Self {
        MantraError::Parse(crate::parser::error::ParseError::ParseFailed)
    }

    pub fn actor(msg: impl Into<String>) -> Self {
        MantraError::Actor(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        MantraError::Config(crate::config::error::ConfigError::Invalid(msg.into()))
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        MantraError::Config(crate::config::error::ConfigError::NotFound(msg.into()))
    }
}
