use thiserror::Error;

/// Parser-specific errors
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Failed to set language: {0}")]
    LanguageSetupFailed(String),

    #[error("Failed to parse source code")]
    ParseFailed,

    #[error("Invalid syntax at line {line}: {message}")]
    SyntaxError { line: usize, message: String },

    #[error("Target not found: {0}")]
    TargetNotFound(String),

    #[error("Invalid checksum")]
    InvalidChecksum,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ParseError>;
