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

	"github.com/rail44/mantra/internal/log"
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
	logger             log.Logger
	firstRequestLogged bool // Flag to log detailed info only on first request
}

// OpenAIRequest represents a chat completion request
type OpenAIRequest struct {
	Model             string          `json:"model"`
	Messages          []OpenAIMessage `json:"messages"`
	Temperature       float32         `json:"temperature"`
	Tools             []Tool          `json:"tools,omitempty"`
	ToolChoice        interface{}     `json:"tool_choice,omitempty"`
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

// NewOpenAIClient creates a new OpenAI API client
func NewOpenAIClient(apiKey, baseURL, model string, logger log.Logger) (*OpenAIClient, error) {
	// API key is optional for some providers (e.g., local Ollama)
	if baseURL == "" {
		return nil, fmt.Errorf("base URL is required")
	}

	if logger == nil {
		logger = log.Default()
	}

	return &OpenAIClient{
		apiKey:             apiKey,
		baseURL:            strings.TrimSuffix(baseURL, "/"),
		model:              model,
		currentTemperature: 0.7, // Default temperature
		systemPrompt:       "",  // Will be set by phase
		httpClient: &http.Client{
			Timeout: 5 * time.Minute,
		},
		logger: logger,
	}, nil
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
	// Get logger from context or use the default
	logger := LoggerFromContext(ctx, c.logger)

	jsonData, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal request: %w", err)
	}

	// Log request summary instead of full JSON
	logger.Trace(fmt.Sprintf("[API] Request: %s (msgs=%d, tools=%d, temp=%.2f)",
		req.Model, len(req.Messages), len(req.Tools), req.Temperature))

	// Log provider info only on first request to reduce noise
	if !c.firstRequestLogged && c.providerSpec != nil {
		logger.Trace("sending request with provider spec", slog.String("provider_spec", fmt.Sprintf("%+v", c.providerSpec)))
		c.firstRequestLogged = true
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

	// Log provider info only once per client to reduce noise
	if result.Provider != "" && !c.firstRequestLogged {
		logger.Debug("OpenRouter provider", "provider", result.Provider)
	}

	return &result, nil
}
