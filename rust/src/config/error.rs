use thiserror::Error;

/// Configuration-specific errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration file not found in {0} or any parent directory")]
    NotFound(String),

    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid API key format: {0}")]
    InvalidApiKey(String),

    #[error("Failed to read configuration file: {0}")]
    ReadFailed(String),

    #[error("Failed to parse TOML: {0}")]
    ParseFailed(#[from] toml::de::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ConfigError>;
