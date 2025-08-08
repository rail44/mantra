package config

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/BurntSushi/toml"
)

// Config represents the complete configuration for mantra
type Config struct {
	// Required fields
	Model string `toml:"model"`
	URL   string `toml:"url"`
	Dest  string `toml:"dest"`

	// Optional fields
	APIKey   string `toml:"api_key"`
	LogLevel string `toml:"log_level"`
	Verbose  bool   `toml:"verbose"` // CLI flag, not from config file

	// OpenRouter configuration
	OpenRouter *OpenRouterConfig `toml:"openrouter"`
}

// OpenRouterConfig represents OpenRouter-specific configuration
type OpenRouterConfig struct {
	Providers []string `toml:"providers"`
}

// Load loads configuration from mantra.toml
func Load(targetPath string) (*Config, error) {
	// Find config file starting from target directory
	configPath, err := findConfigFile(targetPath)
	if err != nil {
		return nil, err
	}

	// Read and parse config file
	configData, err := os.ReadFile(configPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read config file: %w", err)
	}

	// Parse TOML
	var cfg Config
	if _, err := toml.Decode(string(configData), &cfg); err != nil {
		return nil, fmt.Errorf("failed to parse config file: %w", err)
	}

	// Validate and warn about hardcoded API keys (before expansion)
	if cfg.APIKey != "" && !strings.Contains(cfg.APIKey, "${") && strings.HasPrefix(cfg.APIKey, "sk-") {
		fmt.Fprintf(os.Stderr, "Warning: API key appears to be hardcoded in mantra.toml. Consider using environment variables: api_key = \"${OPENROUTER_API_KEY}\"\n")
	}

	// Validate required fields
	if err := cfg.validate(); err != nil {
		return nil, err
	}

	// Normalize paths
	cfg.Dest = normalizePath(cfg.Dest, filepath.Dir(configPath))

	return &cfg, nil
}

// findConfigFile searches for mantra.toml starting from the given path
func findConfigFile(startPath string) (string, error) {
	// Convert to absolute path
	absPath, err := filepath.Abs(startPath)
	if err != nil {
		return "", fmt.Errorf("failed to get absolute path: %w", err)
	}

	// If startPath is a file, start from its directory
	info, err := os.Stat(absPath)
	if err == nil && !info.IsDir() {
		absPath = filepath.Dir(absPath)
	}

	// Search upward for mantra.toml
	currentDir := absPath
	for {
		configPath := filepath.Join(currentDir, "mantra.toml")
		if _, err := os.Stat(configPath); err == nil {
			return configPath, nil
		}

		// Move to parent directory
		parentDir := filepath.Dir(currentDir)
		if parentDir == currentDir {
			// Reached root directory
			break
		}
		currentDir = parentDir
	}

	return "", fmt.Errorf("mantra.toml not found. Create one with:\n\nmodel = \"devstral\"\nurl = \"http://localhost:11434/v1\"\ndest = \"./generated\"\n\nSee: https://github.com/rail44/mantra#configuration")
}

// expandEnvVars expands ${VAR_NAME} environment variables in the string
func expandEnvVars(s string) string {
	// Pattern to match ${VAR_NAME}
	re := regexp.MustCompile(`\$\{([^}]+)\}`)

	return re.ReplaceAllStringFunc(s, func(match string) string {
		// Extract variable name
		varName := match[2 : len(match)-1]

		// Get environment variable value
		value := os.Getenv(varName)
		if value == "" {
			// Keep original if not set (will be caught in validation)
			return match
		}

		return value
	})
}

// validate checks that all required fields are set
func (c *Config) validate() error {
	var errors []string

	if c.Model == "" {
		errors = append(errors, "model is required")
	}
	if c.URL == "" {
		errors = append(errors, "url is required")
	}
	if c.Dest == "" {
		errors = append(errors, "dest is required")
	}

	// Check for unexpanded environment variables
	if strings.Contains(c.APIKey, "${") {
		// Try to expand and check if the environment variable exists
		expanded := expandEnvVars(c.APIKey)
		if strings.Contains(expanded, "${") {
			// Still contains ${}, so the env var is not set
			re := regexp.MustCompile(`\$\{([^}]+)\}`)
			matches := re.FindStringSubmatch(c.APIKey)
			if len(matches) > 1 {
				return fmt.Errorf("environment variable %s is not set (required by api_key in mantra.toml)", matches[1])
			}
		}
	}

	// API key warning is already handled in LoadConfig

	if len(errors) > 0 {
		return fmt.Errorf("invalid configuration: %s", strings.Join(errors, ", "))
	}

	return nil
}

// normalizePath converts relative paths to absolute paths based on config file location
func normalizePath(path, configDir string) string {
	if filepath.IsAbs(path) {
		return path
	}
	return filepath.Join(configDir, path)
}

// GetPackageName returns the package name based on the destination directory
func (c *Config) GetPackageName() string {
	return filepath.Base(c.Dest)
}

// GetAPIKey returns the API key with environment variables expanded
func (c *Config) GetAPIKey() string {
	if c.APIKey == "" {
		return ""
	}

	// Expand environment variables
	return expandEnvVars(c.APIKey)
}
