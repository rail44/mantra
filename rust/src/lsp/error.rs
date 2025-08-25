use thiserror::Error;

/// LSP-specific errors
#[derive(Error, Debug)]
pub enum LspError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
