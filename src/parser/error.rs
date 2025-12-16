use thiserror::Error;

/// Parser-specific errors
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Failed to parse source code")]
    ParseFailed,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
