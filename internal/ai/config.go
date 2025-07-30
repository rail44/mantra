package ai

import (
	"os"
	"time"
)

type Config struct {
	Model        string
	Host         string        // Base URL for the API endpoint
	Timeout      time.Duration
	Temperature  float32
	SystemPrompt string
	Provider     string  // Provider type: "ollama", "openai", or custom
	APIKey       string  // API key for providers that require authentication
}

func DefaultConfig() *Config {
	return &Config{
		Model:       "devstral",
		Host:        getEnvOrDefault("OLLAMA_HOST", "http://localhost:11434"),
		Timeout:     5 * time.Minute,
		Temperature: 0.7,
		SystemPrompt: `You are an expert Go developer generating function implementations.
CRITICAL: You must generate ONLY the code that goes INSIDE the function body braces.
Do NOT include:
- Function signature (func name(params) returns)
- Opening or closing braces of the function
- Package declaration
- Import statements
- Comments outside the function body

Example correct response for "add two numbers":
return a + b

Example INCORRECT response (DO NOT DO THIS):
func Add(a, b int) int {
    return a + b
}

Requirements:
- Follow Go best practices and idioms
- Handle edge cases appropriately
- When the instruction is in Japanese, understand and implement according to the full specification
- For functions that return errors, use proper error handling
- For string operations, handle Unicode correctly`,
	}
}

func getEnvOrDefault(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}
