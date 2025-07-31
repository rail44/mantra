package ai

import (
	"context"
	"fmt"
	"os"
)

type Client struct {
	provider         Provider
	clientConfig     *ClientConfig
	generationConfig *GenerationConfig
	debugTiming      bool
	tools            []Tool
	toolExecutor     ToolExecutor
}

func NewClient(clientConfig *ClientConfig, generationConfig *GenerationConfig) (*Client, error) {
	if clientConfig == nil {
		return nil, fmt.Errorf("clientConfig is required")
	}
	if generationConfig == nil {
		return nil, fmt.Errorf("generationConfig is required")
	}

	// Determine provider based on configuration
	var provider Provider
	var err error

	// Unified OpenAI-compatible API for all providers
	apiKey := clientConfig.APIKey
	if apiKey == "" {
		apiKey = os.Getenv("MANTRA_OPENAI_API_KEY")
	}
	
	baseURL := clientConfig.BaseURL
	if baseURL == "" {
		return nil, fmt.Errorf("base URL is required")
	}

	provider, err = NewOpenAIClient(
		apiKey,
		baseURL,
		clientConfig.Model,
		generationConfig.Temperature,
		generationConfig.SystemPrompt,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to create AI client: %w", err)
	}

	return &Client{
		provider:         provider,
		clientConfig:     clientConfig,
		generationConfig: generationConfig,
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

// SetTools sets the tools available for the AI to use
func (c *Client) SetTools(tools []Tool, executor ToolExecutor) {
	c.tools = tools
	c.toolExecutor = executor
}

// GenerateWithTools sends a prompt to the AI with tool support
func (c *Client) GenerateWithTools(ctx context.Context, prompt string) (string, error) {
	// Check if provider supports tools
	toolProvider, ok := c.provider.(ToolProvider)
	if !ok {
		// Fallback to regular generation
		return c.provider.Generate(ctx, prompt)
	}

	// Generate with tools
	return toolProvider.GenerateWithTools(ctx, prompt, c.tools, c.toolExecutor)
}
