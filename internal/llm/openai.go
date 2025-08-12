package llm

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"strings"
	"time"
)

// OpenAIClient implements Provider for OpenAI API and compatible services
type OpenAIClient struct {
	apiKey             string
	baseURL            string
	model              string
	currentTemperature float32 // Current temperature to use
	systemPrompt       string  // Current system prompt
	httpClient         *http.Client
	providerSpec       *ProviderSpec // OpenRouter-specific provider routing
	logger             *slog.Logger
}

// OpenAIRequest represents a chat completion request
type OpenAIRequest struct {
	Model             string          `json:"model"`
	Messages          []OpenAIMessage `json:"messages"`
	Temperature       float32         `json:"temperature"`
	Tools             []Tool          `json:"tools,omitempty"`
	ToolChoice        any             `json:"tool_choice,omitempty"`
	ParallelToolCalls bool            `json:"parallel_tool_calls,omitempty"`
	Provider          *ProviderSpec   `json:"provider,omitempty"` // OpenRouter provider specification
}

// ProviderSpec allows specifying provider routing for OpenRouter
type ProviderSpec struct {
	Only []string `json:"only,omitempty"` // List of providers to use (e.g., ["Cerebras"])
}

// OpenAIMessage represents a message in the chat
type OpenAIMessage struct {
	Role       string     `json:"role"`
	Content    string     `json:"content"`
	Reasoning  string     `json:"reasoning,omitempty"` // For models that support reasoning
	ToolCalls  []ToolCall `json:"tool_calls,omitempty"`
	ToolCallID string     `json:"tool_call_id,omitempty"`
}

// OpenAIResponse represents a chat completion response
type OpenAIResponse struct {
	ID      string `json:"id"`
	Object  string `json:"object"`
	Created int64  `json:"created"`
	Model   string `json:"model"`
	Choices []struct {
		Index        int           `json:"index"`
		Message      OpenAIMessage `json:"message"`
		FinishReason string        `json:"finish_reason"`
	} `json:"choices"`
	Usage struct {
		PromptTokens     int `json:"prompt_tokens"`
		CompletionTokens int `json:"completion_tokens"`
		TotalTokens      int `json:"total_tokens"`
	} `json:"usage"`
	Provider string `json:"provider,omitempty"` // OpenRouter provider info
}

// OpenAIClientOptions contains options for creating an OpenAI client
type OpenAIClientOptions struct {
	APIKey       string
	BaseURL      string
	Model        string
	Temperature  float32
	SystemPrompt string
	HTTPClient   *http.Client
	ProviderSpec []string // For OpenRouter provider routing
	Logger       *slog.Logger
}

// NewOpenAIClient creates a new OpenAI API client
func NewOpenAIClient(apiKey, baseURL, model string, logger *slog.Logger) (*OpenAIClient, error) {
	return NewOpenAIClientWithOptions(&OpenAIClientOptions{
		APIKey:      apiKey,
		BaseURL:     baseURL,
		Model:       model,
		Temperature: 0.7, // Default temperature
		Logger:      logger,
	})
}

// NewOpenAIClientWithOptions creates a new OpenAI API client with full options
func NewOpenAIClientWithOptions(opts *OpenAIClientOptions) (*OpenAIClient, error) {
	if opts.BaseURL == "" {
		return nil, fmt.Errorf("base URL is required")
	}

	if opts.Logger == nil {
		opts.Logger = slog.Default()
	}

	httpClient := opts.HTTPClient
	if httpClient == nil {
		httpClient = &http.Client{
			Timeout: 5 * time.Minute,
		}
	}

	client := &OpenAIClient{
		apiKey:             opts.APIKey,
		baseURL:            strings.TrimSuffix(opts.BaseURL, "/"),
		model:              opts.Model,
		currentTemperature: opts.Temperature,
		systemPrompt:       opts.SystemPrompt,
		httpClient:         httpClient,
		logger:             opts.Logger,
	}

	// Set provider spec if provided
	if len(opts.ProviderSpec) > 0 {
		client.providerSpec = &ProviderSpec{
			Only: opts.ProviderSpec,
		}
	}

	return client, nil
}

// SetProviderSpec sets OpenRouter provider routing specification
func (c *OpenAIClient) SetProviderSpec(providers []string) {
	if len(providers) > 0 {
		c.providerSpec = &ProviderSpec{
			Only: providers,
		}
	}
}

// SetTemperature sets the temperature for generation
func (c *OpenAIClient) SetTemperature(temperature float32) {
	c.currentTemperature = temperature
}

// SetSystemPrompt sets the system prompt
func (c *OpenAIClient) SetSystemPrompt(systemPrompt string) {
	c.systemPrompt = systemPrompt
	// Logging is deferred to Generate() where we have access to the context
}

// Name returns the provider name
func (c *OpenAIClient) Name() string {
	// Return a simple name based on the model being used
	return "OpenAI API"
}

// makeRequest makes a non-streaming request to the API
func (c *OpenAIClient) makeRequest(ctx context.Context, req OpenAIRequest) (*OpenAIResponse, error) {
	jsonData, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal request: %w", err)
	}

	httpReq, err := http.NewRequestWithContext(ctx, "POST", c.baseURL+"/chat/completions", bytes.NewBuffer(jsonData))
	if err != nil {
		return nil, fmt.Errorf("failed to create request: %w", err)
	}

	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Header.Set("Authorization", "Bearer "+c.apiKey)

	// Add app identification headers (primarily for OpenRouter, but safe for all providers)
	// These headers help with app discovery on platforms that support them
	httpReq.Header.Set("HTTP-Referer", "https://github.com/rail44/mantra")
	httpReq.Header.Set("X-Title", "mantra")

	resp, err := c.httpClient.Do(httpReq)
	if err != nil {
		return nil, fmt.Errorf("request failed: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("API request failed with status %d: %s", resp.StatusCode, string(body))
	}

	// Read the response body
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("failed to read response body: %w", err)
	}

	var result OpenAIResponse
	if err := json.Unmarshal(body, &result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &result, nil
}
