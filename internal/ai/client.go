package ai

import (
	"context"
	"fmt"
	"time"
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
}

func NewClient(clientConfig *ClientConfig) (*Client, error) {
	if clientConfig == nil {
		return nil, fmt.Errorf("clientConfig is required")
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
