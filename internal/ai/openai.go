package ai

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/rail44/mantra/internal/log"
)

// OpenAIClient implements Provider for OpenAI API and compatible services
type OpenAIClient struct {
	apiKey            string
	baseURL           string
	model             string
	currentTemperature float32  // Current temperature to use
	systemPrompt      string   // Current system prompt
	httpClient        *http.Client
	debugTiming       bool
	providerSpec      *ProviderSpec // OpenRouter-specific provider routing
}

// OpenAIRequest represents a chat completion request
type OpenAIRequest struct {
	Model             string          `json:"model"`
	Messages          []OpenAIMessage `json:"messages"`
	Temperature       float32         `json:"temperature"`
	Stream            bool            `json:"stream"`
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
	Provider string `json:"provider,omitempty"` // OpenRouter provider info
}

// NewOpenAIClient creates a new OpenAI API client
func NewOpenAIClient(apiKey, baseURL, model string) (*OpenAIClient, error) {
	// API key is optional for some providers (e.g., local Ollama)
	if baseURL == "" {
		return nil, fmt.Errorf("base URL is required")
	}

	return &OpenAIClient{
		apiKey:            apiKey,
		baseURL:           strings.TrimSuffix(baseURL, "/"),
		model:             model,
		currentTemperature: 0.7, // Default temperature
		systemPrompt:      ToolEnabledSystemPrompt(), // Default system prompt
		httpClient: &http.Client{
			Timeout: 5 * time.Minute,
		},
	}, nil
}

// SetDebugTiming enables detailed timing information
func (c *OpenAIClient) SetDebugTiming(enabled bool) {
	c.debugTiming = enabled
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
}

// Name returns the provider name
func (c *OpenAIClient) Name() string {
	// Return a simple name based on the model being used
	return "OpenAI API"
}

// CheckModel verifies if the specified model is available
func (c *OpenAIClient) CheckModel(ctx context.Context) error {
	// Model validation is fast, no need to track timing

	// For OpenAI APIs, we'll do a simple completion request
	// with minimal tokens to verify the model is accessible
	req := OpenAIRequest{
		Model: c.model,
		Messages: []OpenAIMessage{
			{Role: "user", Content: "test"},
		},
		Temperature: 0,
		Stream:      false,
		Provider:    c.providerSpec,
	}

	_, err := c.makeRequest(ctx, req)

	if c.debugTiming {
		// Model check timing is included in overall metrics
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

	// Log request summary instead of full JSON
	log.Trace(fmt.Sprintf("[API] Request: %s (msgs=%d, tools=%d, temp=%.2f)",
		req.Model, len(req.Messages), len(req.Tools), req.Temperature))

	// Debug: Log request with provider info
	if c.providerSpec != nil {
		log.Debug("sending request with provider spec", "provider_spec", fmt.Sprintf("%+v", c.providerSpec))
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

	// Log provider info if available (OpenRouter)
	if result.Provider != "" {
		log.Debug("OpenRouter provider", "provider", result.Provider)
	}

	return &result, nil
}

// Generate sends a prompt with tool definitions and handles tool calls
func (c *OpenAIClient) Generate(ctx context.Context, prompt string, tools []Tool, executor ToolExecutor) (string, error) {
	overallStart := time.Now()
	var toolExecutionTime time.Duration
	var apiCallTime time.Duration
	var toolCallCount int
	

	// Build initial messages with system prompt
	messages := []OpenAIMessage{
		{
			Role:    "system",
			Content: c.systemPrompt,
		},
		{
			Role:    "user",
			Content: prompt,
		},
	}

	// Maximum rounds of tool calls to prevent infinite loops
	const maxRounds = 30

	for round := 0; round < maxRounds; round++ {
		if round > 0 {
			log.Debug("tool usage round", "round", round+1, "max_rounds", maxRounds)
		}

		// Log concise message stats
		log.Debug(fmt.Sprintf("[ROUND] %d/%d: %d messages", round+1, maxRounds, len(messages)))

		// Use the current temperature set by the phase
		temperature := c.currentTemperature

		req := OpenAIRequest{
			Model:             c.model,
			Messages:          messages,
			Temperature:       temperature,
			Stream:            false,
			Tools:             tools,
			ToolChoice:        "auto",
			ParallelToolCalls: true,
			Provider:          c.providerSpec,
		}

		// Make API call
		apiStart := time.Now()
		resp, err := c.makeRequest(ctx, req)
		apiCallTime += time.Since(apiStart)
		if err != nil {
			return "", err
		}

		if len(resp.Choices) == 0 {
			return "", fmt.Errorf("no response choices returned")
		}

		responseMsg := resp.Choices[0].Message

		// Fix missing Type field for Mistral API compatibility
		for i := range responseMsg.ToolCalls {
			if responseMsg.ToolCalls[i].Type == "" {
				responseMsg.ToolCalls[i].Type = "function"
			}
		}

		// Create clean message with only required fields to avoid API compatibility issues
		cleanMsg := OpenAIMessage{
			Role:      responseMsg.Role,
			Content:   responseMsg.Content,
			ToolCalls: responseMsg.ToolCalls,
		}
		messages = append(messages, cleanMsg)

		// Debug: Log response
		log.Debug(fmt.Sprintf("[API] Response: %s (content=%d chars, tools=%d)",
			responseMsg.Role, len(responseMsg.Content), len(responseMsg.ToolCalls)))
		if round >= 5 && len(responseMsg.ToolCalls) > 0 {
			log.Warn("many tool calls made - model may be stuck", "round", round+1)
		}

		// Check if we have tool calls
		if len(responseMsg.ToolCalls) == 0 {
			// No tool calls, return the content
			totalTime := time.Since(overallStart)
			log.Debug("tool usage completed", "rounds", round+1)
			log.Debug(fmt.Sprintf("[PERF] Total: %s (API=%.1f%%, tools=%.1f%%, %d tool calls)",
				totalTime.Round(time.Millisecond),
				float64(apiCallTime)/float64(totalTime)*100,
				float64(toolExecutionTime)/float64(totalTime)*100,
				toolCallCount))
			// Average tool time is shown in main performance log if relevant
			log.Debug("final response", "content_length", len(responseMsg.Content))
			// Only log content at TRACE level if it's reasonably sized
			if log.IsTraceEnabled() && len(responseMsg.Content) < 500 {
				log.Trace(fmt.Sprintf("[CONTENT] Response: %s", responseMsg.Content))
			}
			return responseMsg.Content, nil
		}

		// Mark that we've used tools in this session
		

		// Execute tool calls in parallel
		toolResults := c.executeToolsParallel(ctx, responseMsg.ToolCalls, executor, &toolExecutionTime, &toolCallCount)

		// Add all tool responses to messages
		messages = append(messages, toolResults...)

	}

	return "", fmt.Errorf("exceeded maximum rounds of tool calls")
}

// executeToolsParallel executes multiple tool calls in parallel
func (c *OpenAIClient) executeToolsParallel(ctx context.Context, toolCalls []ToolCall, executor ToolExecutor, toolExecutionTime *time.Duration, toolCallCount *int) []OpenAIMessage {
	type toolResult struct {
		index      int
		toolCallID string
		message    OpenAIMessage
		duration   time.Duration
	}

	results := make(chan toolResult, len(toolCalls))
	var wg sync.WaitGroup

	// Execute all tools in parallel
	for i, toolCall := range toolCalls {
		wg.Add(1)
		go func(index int, tc ToolCall) {
			defer wg.Done()

			// Parse parameters

			// Check if Arguments is already a string (double-encoded)
			var argStr string
			if err := json.Unmarshal(tc.Function.Arguments, &argStr); err == nil {
				// It was double-encoded, use the decoded string
				tc.Function.Arguments = json.RawMessage(argStr)
			}

			var params map[string]interface{}
			if err := json.Unmarshal(tc.Function.Arguments, &params); err != nil {
				results <- toolResult{
					index:      index,
					toolCallID: tc.ID,
					message: OpenAIMessage{
						Role:       "tool",
						Content:    fmt.Sprintf("Error parsing arguments: %v", err),
						ToolCallID: tc.ID,
					},
				}
				return
			}

			// Log tool execution
			log.Debug(fmt.Sprintf("[TOOL] Calling: %s(%v)", tc.Function.Name, params))

			// Execute tool
			toolStart := time.Now()
			result, err := executor.Execute(ctx, tc.Function.Name, params)
			duration := time.Since(toolStart)

			if err != nil {
				results <- toolResult{
					index:      index,
					toolCallID: tc.ID,
					duration:   duration,
					message: OpenAIMessage{
						Role:       "tool",
						Content:    fmt.Sprintf("Error executing tool: %v", err),
						ToolCallID: tc.ID,
					},
				}
				return
			}

			// Marshal result
			resultJSON, err := json.Marshal(result)
			if err != nil {
				results <- toolResult{
					index:      index,
					toolCallID: tc.ID,
					duration:   duration,
					message: OpenAIMessage{
						Role:       "tool",
						Content:    fmt.Sprintf("Error marshaling result: %v", err),
						ToolCallID: tc.ID,
					},
				}
				return
			}

			// Log tool result concisely
			if log.IsTraceEnabled() {
				resultStr := string(resultJSON)
				if len(resultStr) > 200 {
					resultStr = resultStr[:200] + "..."
				}
				log.Trace(fmt.Sprintf("[TOOL] Result %s: %s", tc.Function.Name, resultStr))
			}

			results <- toolResult{
				index:      index,
				toolCallID: tc.ID,
				duration:   duration,
				message: OpenAIMessage{
					Role:       "tool",
					Content:    string(resultJSON),
					ToolCallID: tc.ID,
				},
			}
		}(i, toolCall)
	}

	// Wait for all tools to complete
	go func() {
		wg.Wait()
		close(results)
	}()

	// Collect results in order
	resultSlice := make([]toolResult, 0, len(toolCalls))
	for result := range results {
		resultSlice = append(resultSlice, result)
		*toolExecutionTime += result.duration
		*toolCallCount++
		// Tool timing is included in overall performance metrics
	}

	// Sort by original index to maintain order
	sort.Slice(resultSlice, func(i, j int) bool {
		return resultSlice[i].index < resultSlice[j].index
	})

	// Extract messages in order
	messages := make([]OpenAIMessage, len(resultSlice))
	for i, result := range resultSlice {
		messages[i] = result.message
	}

	return messages
}
