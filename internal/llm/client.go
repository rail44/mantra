package llm

import (
	"context"
	"fmt"
	"log/slog"
	"net/http"
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
	logger       *slog.Logger
}

func NewClient(clientConfig *ClientConfig, httpClient *http.Client, logger *slog.Logger) (*Client, error) {
	if clientConfig == nil {
		return nil, fmt.Errorf("clientConfig is required")
	}

	// Use default logger if not provided
	if logger == nil {
		logger = slog.Default()
	}

	url := clientConfig.URL
	if url == "" {
		return nil, fmt.Errorf("URL is required")
	}

	// Create provider with provided HTTP client
	opts := &OpenAIClientOptions{
		APIKey:       clientConfig.APIKey,
		BaseURL:      url,
		Model:        clientConfig.Model,
		Temperature:  0.7,        // Default, will be overridden by phase
		HTTPClient:   httpClient, // Can be nil, will be created if needed
		ProviderSpec: clientConfig.Provider,
		Logger:       logger,
	}

	provider, err := NewOpenAIClientWithOptions(opts)
	if err != nil {
		return nil, fmt.Errorf("failed to create AI client: %w", err)
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

// GetConfig returns the client configuration
func (c *Client) GetConfig() *ClientConfig {
	return c.clientConfig
}
