pub mod error;

use crate::core::{MantraError, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Main configuration structure for Mantra
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Model to use for code generation (required)
    pub model: String,

    /// API endpoint URL (required)
    pub url: String,

    /// API key for authentication (optional)
    pub api_key: Option<String>,

    /// Log level: error, warn, info, debug, trace
    pub log_level: Option<String>,

    /// OpenRouter-specific configuration
    pub openrouter: Option<OpenRouterConfig>,
}

/// OpenRouter-specific configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenRouterConfig {
    /// List of providers to route to
    pub providers: Vec<String>,
}

impl Config {
    /// Load configuration from mantra.toml
    pub fn load(target_path: impl AsRef<Path>) -> Result<Self> {
        // Find config file starting from target directory
        let config_path = find_config_file(target_path.as_ref()).ok_or_else(|| {
            MantraError::config(format!(
                "Failed to find mantra.toml in {} or any parent directory",
                target_path.as_ref().display()
            ))
        })?;

        tracing::info!("Found configuration at: {}", config_path.display());

        // Read config file
        let config_data = fs::read_to_string(&config_path).map_err(|e| {
            MantraError::config(format!(
                "Failed to read config file {}: {}",
                config_path.display(),
                e
            ))
        })?;

        // Parse TOML
        let mut config: Config = toml::from_str(&config_data)
            .map_err(|e| MantraError::config(format!("Failed to parse mantra.toml: {}", e)))?;

        // Warn about hardcoded API keys
        if let Some(api_key) = &config.api_key {
            if !api_key.contains("${") && api_key.starts_with("sk-") {
                warn!("API key appears to be hardcoded in mantra.toml. Consider using environment variables: api_key = \"${{OPENROUTER_API_KEY}}\"");
            }
        }

        // Expand environment variables
        config.expand_env_vars();

        // Validate required fields
        config.validate()?;

        Ok(config)
    }

    /// Validate that required fields are present
    fn validate(&self) -> Result<()> {
        if self.model.is_empty() {
            return Err(MantraError::config("'model' is required in mantra.toml"));
        }
        if self.url.is_empty() {
            return Err(MantraError::config("'url' is required in mantra.toml"));
        }
        Ok(())
    }

    /// Expand environment variables in configuration values
    fn expand_env_vars(&mut self) {
        // Expand API key if it contains ${...}
        if let Some(api_key) = &self.api_key {
            if let Some(expanded) = expand_env_var(api_key) {
                self.api_key = Some(expanded);
            }
        }

        // Could expand other fields if needed
    }
}

/// Find mantra.toml by searching upward from the given path
fn find_config_file(start_path: &Path) -> Option<PathBuf> {
    let current_dir = if start_path.is_file() {
        start_path.parent()?
    } else {
        start_path
    };

    // Convert to absolute path
    let mut current_dir = current_dir.canonicalize().ok()?;

    loop {
        let config_path = current_dir.join("mantra.toml");
        if config_path.exists() {
            return Some(config_path);
        }

        // Move to parent directory
        match current_dir.parent() {
            Some(parent) => current_dir = parent.to_path_buf(),
            None => break, // Reached root
        }
    }

    None
}

/// Expand environment variable in the format ${VAR_NAME}
fn expand_env_var(value: &str) -> Option<String> {
    // Check for ${...} pattern
    if value.starts_with("${") && value.ends_with('}') {
        let var_name = &value[2..value.len() - 1];
        env::var(var_name).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_expand_env_var() {
        env::set_var("TEST_VAR", "test_value");

        // Test ${VAR} format
        assert_eq!(
            expand_env_var("${TEST_VAR}"),
            Some("test_value".to_string())
        );

        // Test $VAR format
        assert_eq!(expand_env_var("$TEST_VAR"), Some("test_value".to_string()));

        // Test env:VAR format
        assert_eq!(
            expand_env_var("env:TEST_VAR"),
            Some("test_value".to_string())
        );

        // Test non-existent variable
        assert_eq!(expand_env_var("${NONEXISTENT}"), None);
        assert_eq!(expand_env_var("$NONEXISTENT"), None);
        assert_eq!(expand_env_var("env:NONEXISTENT"), None);

        // Test non-variable string
        assert_eq!(expand_env_var("not_a_var"), None);

        env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config {
            model: "test-model".to_string(),
            url: "http://localhost".to_string(),
            api_key: None,
            log_level: None,
            openrouter: None,
        };

        assert!(config.validate().is_ok());

        config.model = String::new();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_find_config_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("mantra.toml");
        fs::write(
            &config_path,
            "model = \"test\"\nurl = \"test\"\ndest = \"test\"",
        )?;

        // Should find config in same directory
        let found = find_config_file(temp_dir.path())
            .ok_or_else(|| MantraError::not_found("Config file not found"))?;
        assert_eq!(found.canonicalize()?, config_path.canonicalize()?);

        // Should find config from subdirectory
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir)?;
        let found = find_config_file(&sub_dir)
            .ok_or_else(|| MantraError::not_found("Config file not found"))?;
        assert_eq!(found.canonicalize()?, config_path.canonicalize()?);

        Ok(())
    }
}
