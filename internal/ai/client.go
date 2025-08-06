package ai

import (
	"context"
	"fmt"
	"time"

	"github.com/rail44/mantra/internal/log"
)

// ClientConfig represents the configuration for connecting to an AI provider
type ClientConfig struct {
	URL      string        // URL for the API endpoint (e.g., "http://localhost:11434/v1" for Ollama)
	APIKey   string        // API key for providers that require authentication
	Model    string        // Model to use
	Timeout  time.Duration // Request timeout
	Provider []string      // OpenRouter provider specification (e.g., ["Cerebras"])
}

type Client struct {
	provider     Provider
	clientConfig *ClientConfig
	tools        []Tool
	toolExecutor ToolExecutor
	logger       log.Logger
}

func NewClient(clientConfig *ClientConfig, logger log.Logger) (*Client, error) {
	if clientConfig == nil {
		return nil, fmt.Errorf("clientConfig is required")
	}

	// Use default logger if not provided
	if logger == nil {
		logger = log.Default()
	}

	// Determine provider based on configuration
	var provider Provider
	var err error

	// Unified OpenAI-compatible API for all providers
	apiKey := clientConfig.APIKey

	url := clientConfig.URL
	if url == "" {
		return nil, fmt.Errorf("URL is required")
	}

	provider, err = NewOpenAIClient(
		apiKey,
		url,
		clientConfig.Model,
		logger,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to create AI client: %w", err)
	}

	// Set provider specification if it's an OpenAIClient and providers are specified
	if openaiClient, ok := provider.(*OpenAIClient); ok && len(clientConfig.Provider) > 0 {
		openaiClient.SetProviderSpec(clientConfig.Provider)
	}

	return &Client{
		provider:     provider,
		clientConfig: clientConfig,
		logger:       logger,
	}, nil
}

// Generate sends a prompt to the AI with tool support
func (c *Client) Generate(ctx context.Context, prompt string) (string, error) {
	return c.provider.Generate(ctx, prompt, c.tools, c.toolExecutor)
}

// GetProviderName returns the name of the current provider
func (c *Client) GetProviderName() string {
	return c.provider.Name()
}

// SetTools sets the tools available for the AI to use
func (c *Client) SetTools(tools []Tool, executor ToolExecutor) {
	c.tools = tools
	c.toolExecutor = executor
}

// SetTemperature sets the temperature for generation
func (c *Client) SetTemperature(temperature float32) {
	c.provider.SetTemperature(temperature)
}

// SetSystemPrompt sets the system prompt
func (c *Client) SetSystemPrompt(systemPrompt string) {
	c.provider.SetSystemPrompt(systemPrompt)
}

// SetLogger sets the logger for the client
func (c *Client) SetLogger(logger log.Logger) {
	c.logger = logger
	// Also update provider's logger if it's an OpenAIClient
	if openaiClient, ok := c.provider.(*OpenAIClient); ok {
		openaiClient.logger = logger
	}
}

// GetConfig returns the client configuration
func (c *Client) GetConfig() *ClientConfig {
	return c.clientConfig
}
