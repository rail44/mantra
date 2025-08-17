use thiserror::Error;

/// LSP-specific errors
#[derive(Error, Debug)]
pub enum LspError {
    #[error("Failed to connect to language server: {0}")]
    ConnectionFailed(String),

    #[error("Language server not responding")]
    ServerNotResponding,

    #[error("Invalid response from language server: {0}")]
    InvalidResponse(String),

    #[error("Method not supported: {0}")]
    MethodNotSupported(String),

    #[error("JSON-RPC error: {0}")]
    JsonRpc(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, LspError>;
