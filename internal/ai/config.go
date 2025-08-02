package ai

import (
	"time"
)

// Config is deprecated. Use ClientConfig and GenerationConfig instead.
type Config struct {
	Model       string
	Host        string // Base URL for the API endpoint
	Timeout     time.Duration
	Temperature float32
	Provider    string // Deprecated: provider type
	APIKey      string // API key for providers that require authentication
}

// ClientConfig represents the configuration for connecting to an AI provider
type ClientConfig struct {
	URL      string        // URL for the API endpoint (e.g., "http://localhost:11434/v1" for Ollama)
	APIKey   string        // API key for providers that require authentication
	Model    string        // Model to use
	Timeout  time.Duration // Request timeout
	Provider []string      // OpenRouter provider specification (e.g., ["Cerebras"])
}

// GenerationConfig represents the configuration for content generation
type GenerationConfig struct {
	Temperature float32 // Temperature for generation (0.0 to 1.0)
}

func DefaultConfig() *Config {
	return &Config{
		Timeout:     5 * time.Minute,
		Temperature: 0.7,
	}
}

// DefaultClientConfig returns default client configuration
func DefaultClientConfig() *ClientConfig {
	// No defaults in the simplified version - config is required
	return &ClientConfig{
		Timeout: 5 * time.Minute,
	}
}

// DefaultGenerationConfig returns default generation configuration
func DefaultGenerationConfig() *GenerationConfig {
	config := DefaultConfig()
	return &GenerationConfig{
		Temperature: config.Temperature,
	}
}

// ToolEnabledSystemPrompt returns a system prompt optimized for tool usage
func ToolEnabledSystemPrompt() string {
	return `You are an expert Go developer. Your task: generate ONLY the code that replaces <IMPLEMENT_HERE>.

## Critical Rules
1. Output ONLY the code that goes inside the function body
2. NO function signatures, NO braces, NO markdown, NO explanations
3. If unsure about a type's fields/methods, use inspect() first
4. Always validate with check_syntax() before returning

## Available Tools
- **inspect**: Get struct/interface details (parameter: name)
- **read_func**: See existing implementations (parameter: name) 
- **check_syntax**: Validate your code (parameter: code)
- **search**: Find type definitions (parameter: pattern)

## Process
1. Read the Context section - it may have all type info you need
2. If a type's structure is unclear, use inspect() to avoid errors
3. Generate the implementation
4. Validate with check_syntax()
5. Return ONLY the validated code

## Example
Input: func (c *Cache) Get(key string) interface{} { <IMPLEMENT_HERE> }
Output: value, exists := c.data[key]
if !exists {
    return nil
}
return value`
}
