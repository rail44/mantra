package ai

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// OpenAIClient implements Provider for OpenAI-compatible APIs (including Ollama)
type OpenAIClient struct {
	apiKey      string
	baseURL     string
	model       string
	temperature float32
	systemPrompt string
	httpClient  *http.Client
	debugTiming bool
	providerName string // Track which provider we're actually using
}

// OpenAIRequest represents a chat completion request
type OpenAIRequest struct {
	Model       string         `json:"model"`
	Messages    []OpenAIMessage `json:"messages"`
	Temperature float32        `json:"temperature"`
	Stream      bool          `json:"stream"`
}

// OpenAIMessage represents a message in the chat
type OpenAIMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

// OpenAIResponse represents a chat completion response
type OpenAIResponse struct {
	ID      string `json:"id"`
	Object  string `json:"object"`
	Created int64  `json:"created"`
	Model   string `json:"model"`
	Choices []struct {
		Index   int           `json:"index"`
		Message OpenAIMessage `json:"message"`
		FinishReason string   `json:"finish_reason"`
	} `json:"choices"`
	Usage struct {
		PromptTokens     int `json:"prompt_tokens"`
		CompletionTokens int `json:"completion_tokens"`
		TotalTokens      int `json:"total_tokens"`
	} `json:"usage"`
}

// OpenAIStreamResponse represents a streaming response chunk
type OpenAIStreamResponse struct {
	ID      string `json:"id"`
	Object  string `json:"object"`
	Created int64  `json:"created"`
	Model   string `json:"model"`
	Choices []struct {
		Index int `json:"index"`
		Delta struct {
			Content string `json:"content"`
		} `json:"delta"`
		FinishReason *string `json:"finish_reason"`
	} `json:"choices"`
}

// NewOpenAIClient creates a new OpenAI-compatible client
func NewOpenAIClient(apiKey, baseURL, model string, temperature float32, systemPrompt string) (*OpenAIClient, error) {
	// API key is optional for some providers (e.g., local Ollama)
	if baseURL == "" {
		return nil, fmt.Errorf("base URL is required")
	}

	// Determine provider name from base URL
	providerName := "OpenAI-compatible"
	if strings.Contains(baseURL, "openai.com") {
		providerName = "OpenAI"
	} else if strings.Contains(baseURL, "localhost:11434") {
		providerName = "Ollama"
	}

	return &OpenAIClient{
		apiKey:      apiKey,
		baseURL:     strings.TrimSuffix(baseURL, "/"),
		model:       model,
		temperature: temperature,
		systemPrompt: systemPrompt,
		httpClient: &http.Client{
			Timeout: 5 * time.Minute,
		},
		providerName: providerName,
	}, nil
}

// SetDebugTiming enables detailed timing information
func (c *OpenAIClient) SetDebugTiming(enabled bool) {
	c.debugTiming = enabled
}

// Name returns the provider name
func (c *OpenAIClient) Name() string {
	return c.providerName
}

// Generate sends a prompt to the AI and returns the response
func (c *OpenAIClient) Generate(ctx context.Context, prompt string) (string, error) {
	totalStart := time.Now()

	// Build request
	req := OpenAIRequest{
		Model: c.model,
		Messages: []OpenAIMessage{
			{Role: "system", Content: c.systemPrompt},
			{Role: "user", Content: prompt},
		},
		Temperature: c.temperature,
		Stream:      false,
	}

	if c.debugTiming {
		fmt.Printf("    [AI Timing] Request preparation: %v\n", time.Since(totalStart))
		fmt.Printf("    [AI Timing] Prompt size: %d chars\n", len(prompt))
	}

	// Make API call
	apiCallStart := time.Now()
	resp, err := c.makeRequest(ctx, req)
	if err != nil {
		return "", err
	}

	if c.debugTiming {
		fmt.Printf("    [AI Timing] API call duration: %v\n", time.Since(apiCallStart))
		fmt.Printf("    [AI Timing] Total duration: %v\n", time.Since(totalStart))
		if resp.Usage.TotalTokens > 0 {
			fmt.Printf("    [AI Timing] Tokens used: %d (prompt: %d, completion: %d)\n",
				resp.Usage.TotalTokens, resp.Usage.PromptTokens, resp.Usage.CompletionTokens)
		}
	}

	if len(resp.Choices) == 0 {
		return "", fmt.Errorf("no response choices returned")
	}

	return resp.Choices[0].Message.Content, nil
}

// GenerateStream sends a prompt and returns channels for streaming responses
func (c *OpenAIClient) GenerateStream(ctx context.Context, prompt string) (<-chan string, <-chan error) {
	outputCh := make(chan string, 100)
	errorCh := make(chan error, 1)

	go func() {
		defer close(outputCh)
		defer close(errorCh)

		// Build request
		req := OpenAIRequest{
			Model: c.model,
			Messages: []OpenAIMessage{
				{Role: "system", Content: c.systemPrompt},
				{Role: "user", Content: prompt},
			},
			Temperature: c.temperature,
			Stream:      true,
		}

		// Make streaming request
		err := c.makeStreamRequest(ctx, req, outputCh)
		if err != nil {
			errorCh <- err
		}
	}()

	return outputCh, errorCh
}

// CheckModel verifies if the specified model is available
func (c *OpenAIClient) CheckModel(ctx context.Context) error {
	checkStart := time.Now()

	// For OpenAI-compatible APIs, we'll do a simple completion request
	// with minimal tokens to verify the model is accessible
	req := OpenAIRequest{
		Model: c.model,
		Messages: []OpenAIMessage{
			{Role: "user", Content: "test"},
		},
		Temperature: 0,
		Stream:      false,
	}

	_, err := c.makeRequest(ctx, req)

	if c.debugTiming {
		fmt.Printf("    [AI Timing] Model check: %v\n", time.Since(checkStart))
	}

	if err != nil {
		return fmt.Errorf("model %s check failed: %w", c.model, err)
	}

	return nil
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

	resp, err := c.httpClient.Do(httpReq)
	if err != nil {
		return nil, fmt.Errorf("request failed: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("API request failed with status %d: %s", resp.StatusCode, string(body))
	}

	var result OpenAIResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &result, nil
}

// makeStreamRequest makes a streaming request to the API
func (c *OpenAIClient) makeStreamRequest(ctx context.Context, req OpenAIRequest, outputCh chan<- string) error {
	jsonData, err := json.Marshal(req)
	if err != nil {
		return fmt.Errorf("failed to marshal request: %w", err)
	}

	httpReq, err := http.NewRequestWithContext(ctx, "POST", c.baseURL+"/chat/completions", bytes.NewBuffer(jsonData))
	if err != nil {
		return fmt.Errorf("failed to create request: %w", err)
	}

	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Header.Set("Authorization", "Bearer "+c.apiKey)
	httpReq.Header.Set("Accept", "text/event-stream")

	resp, err := c.httpClient.Do(httpReq)
	if err != nil {
		return fmt.Errorf("request failed: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("API request failed with status %d: %s", resp.StatusCode, string(body))
	}

	scanner := bufio.NewScanner(resp.Body)
	for scanner.Scan() {
		line := scanner.Text()
		
		if !strings.HasPrefix(line, "data: ") {
			continue
		}

		data := strings.TrimPrefix(line, "data: ")
		if data == "[DONE]" {
			return nil
		}

		var chunk OpenAIStreamResponse
		if err := json.Unmarshal([]byte(data), &chunk); err != nil {
			continue // Skip malformed chunks
		}

		if len(chunk.Choices) > 0 && chunk.Choices[0].Delta.Content != "" {
			select {
			case outputCh <- chunk.Choices[0].Delta.Content:
			case <-ctx.Done():
				return ctx.Err()
			}
		}
	}

	if err := scanner.Err(); err != nil {
		return fmt.Errorf("error reading stream: %w", err)
	}

	return nil
}