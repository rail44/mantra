package ai

import (
	"context"
	"fmt"
	"os"
)

type Client struct {
	provider    Provider
	config      *Config
	debugTiming bool
}

func NewClient(config *Config) (*Client, error) {
	if config == nil {
		config = DefaultConfig()
	}

	// Determine provider based on configuration
	var provider Provider
	var err error

	// Check if we should use OpenAI-compatible API
	if config.Provider == "openai" || os.Getenv("MANTRA_OPENAI_API_KEY") != "" {
		apiKey := os.Getenv("MANTRA_OPENAI_API_KEY")
		if apiKey == "" && config.APIKey != "" {
			apiKey = config.APIKey
		}
		
		baseURL := config.Host
		if baseURL == "" {
			baseURL = os.Getenv("MANTRA_OPENAI_BASE_URL")
		}

		provider, err = NewOpenAIClient(
			apiKey,
			baseURL,
			config.Model,
			config.Temperature,
			config.SystemPrompt,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to create OpenAI client: %w", err)
		}
	} else {
		// Default to Ollama
		provider, err = NewOllamaClient(
			config.Host,
			config.Model,
			config.Temperature,
			config.SystemPrompt,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to create Ollama client: %w", err)
		}
	}

	return &Client{
		provider: provider,
		config:   config,
	}, nil
}

// SetDebugTiming enables detailed timing information
func (c *Client) SetDebugTiming(enabled bool) {
	c.debugTiming = enabled
	// Pass through to provider if it supports debug timing
	if debuggable, ok := c.provider.(interface{ SetDebugTiming(bool) }); ok {
		debuggable.SetDebugTiming(enabled)
	}
}

// Generate sends a prompt to the AI and returns the response
func (c *Client) Generate(ctx context.Context, prompt string) (string, error) {
	return c.provider.Generate(ctx, prompt)
}

// GenerateStream sends a prompt and returns a channel for streaming responses
func (c *Client) GenerateStream(ctx context.Context, prompt string) (<-chan string, <-chan error) {
	return c.provider.GenerateStream(ctx, prompt)
}

// CheckModel verifies if the specified model is available
func (c *Client) CheckModel(ctx context.Context) error {
	return c.provider.CheckModel(ctx)
}

// GetProviderName returns the name of the current provider
func (c *Client) GetProviderName() string {
	return c.provider.Name()
}
