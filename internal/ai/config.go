package ai

import (
	"os"
	"time"
)

type Config struct {
	Model        string
	Host         string
	Mode         string        // Generation mode (spanner, generic, etc.)
	Timeout      time.Duration
	Temperature  float32
	SystemPrompt string
}

func DefaultConfig() *Config {
	return &Config{
		Model:       "devstral",
		Host:        getEnvOrDefault("OLLAMA_HOST", "http://localhost:11434"),
		Timeout:     5 * time.Minute,
		Temperature: 0.7,
		SystemPrompt: `You are an expert Go developer specializing in Google Cloud Spanner.
Generate clean, idiomatic Go code that follows best practices.
Focus on creating efficient SQL queries optimized for Spanner's distributed architecture.
Always include proper error handling and context usage.`,
	}
}

func getEnvOrDefault(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}