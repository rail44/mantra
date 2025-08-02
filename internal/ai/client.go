package ai

import (
	"context"
	"fmt"
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

	url := clientConfig.URL
	if url == "" {
		return nil, fmt.Errorf("URL is required")
	}

	provider, err = NewOpenAIClient(
		apiKey,
		url,
		clientConfig.Model,
		generationConfig.Temperature,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to create AI client: %w", err)
	}

	// Set provider specification if it's an OpenAIClient and providers are specified
	if openaiClient, ok := provider.(*OpenAIClient); ok && len(clientConfig.Provider) > 0 {
		openaiClient.SetProviderSpec(clientConfig.Provider)
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

// Generate sends a prompt to the AI with tool support
func (c *Client) Generate(ctx context.Context, prompt string) (string, error) {
	return c.provider.Generate(ctx, prompt, c.tools, c.toolExecutor)
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
