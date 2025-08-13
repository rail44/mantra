use anyhow::{Context, Result};
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
    
    /// Output directory for generated files (required)
    pub dest: String,
    
    /// API key for authentication (optional)
    pub api_key: Option<String>,
    
    /// Log level: error, warn, info, debug, trace
    pub log_level: Option<String>,
    
    /// Plain output mode (from CLI flag, not from config file)
    #[serde(skip)]
    pub plain: bool,
    
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
        let config_path = find_config_file(target_path.as_ref())
            .context("Failed to find mantra.toml")?;
        
        // Read config file
        let config_data = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
        
        // Parse TOML
        let mut config: Config = toml::from_str(&config_data)
            .context("Failed to parse mantra.toml")?;
        
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
        
        // Normalize paths relative to config file location
        let config_dir = config_path.parent().unwrap_or(Path::new("."));
        config.normalize_paths(config_dir);
        
        Ok(config)
    }
    
    /// Validate that required fields are present
    fn validate(&self) -> Result<()> {
        if self.model.is_empty() {
            anyhow::bail!("'model' is required in mantra.toml");
        }
        if self.url.is_empty() {
            anyhow::bail!("'url' is required in mantra.toml");
        }
        if self.dest.is_empty() {
            anyhow::bail!("'dest' is required in mantra.toml");
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
    
    /// Normalize paths to be relative to config file location
    fn normalize_paths(&mut self, config_dir: &Path) {
        // If dest is relative, make it relative to config file location
        if !Path::new(&self.dest).is_absolute() {
            let full_path = config_dir.join(&self.dest);
            self.dest = full_path.to_string_lossy().to_string();
        }
    }
    
    /// Get the package name from the destination directory
    pub fn get_package_name(&self) -> String {
        Path::new(&self.dest)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("generated")
            .to_string()
    }
    
    /// Get the API key, checking environment variable if not in config
    pub fn get_api_key(&self) -> Option<String> {
        self.api_key.clone().or_else(|| {
            // Try common environment variable names
            env::var("OPENROUTER_API_KEY").ok()
                .or_else(|| env::var("OPENAI_API_KEY").ok())
        })
    }
}

/// Find mantra.toml by searching upward from the given path
fn find_config_file(start_path: &Path) -> Result<PathBuf> {
    let current_dir = if start_path.is_file() {
        start_path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
    } else {
        start_path
    };
    
    // Convert to absolute path
    let mut current_dir = current_dir.canonicalize()
        .with_context(|| format!("Failed to resolve path: {}", current_dir.display()))?;
    
    loop {
        let config_path = current_dir.join("mantra.toml");
        if config_path.exists() {
            return Ok(config_path);
        }
        
        // Move to parent directory
        match current_dir.parent() {
            Some(parent) => current_dir = parent.to_path_buf(),
            None => break, // Reached root
        }
    }
    
    anyhow::bail!(
        "Could not find mantra.toml in {} or any parent directory",
        start_path.display()
    )
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
        
        assert_eq!(expand_env_var("${TEST_VAR}"), Some("test_value".to_string()));
        assert_eq!(expand_env_var("${NONEXISTENT}"), None);
        assert_eq!(expand_env_var("not_a_var"), None);
        
        env::remove_var("TEST_VAR");
    }
    
    #[test]
    fn test_config_validation() {
        let mut config = Config {
            model: "test-model".to_string(),
            url: "http://localhost".to_string(),
            dest: "./output".to_string(),
            api_key: None,
            log_level: None,
            plain: false,
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
        fs::write(&config_path, "model = \"test\"\nurl = \"test\"\ndest = \"test\"")?;
        
        // Should find config in same directory
        let found = find_config_file(temp_dir.path())?;
        assert_eq!(found.canonicalize()?, config_path.canonicalize()?);
        
        // Should find config from subdirectory
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir)?;
        let found = find_config_file(&sub_dir)?;
        assert_eq!(found.canonicalize()?, config_path.canonicalize()?);
        
        Ok(())
    }
}