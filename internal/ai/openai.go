package ai

import (
	"bufio"
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
	apiKey      string
	baseURL     string
	model       string
	temperature float32
	systemPrompt string
	httpClient  *http.Client
	debugTiming bool
	providerSpec *ProviderSpec // OpenRouter-specific provider routing
}

// OpenAIRequest represents a chat completion request
type OpenAIRequest struct {
	Model             string         `json:"model"`
	Messages          []OpenAIMessage `json:"messages"`
	Temperature       float32        `json:"temperature"`
	Stream            bool          `json:"stream"`
	Tools             []Tool         `json:"tools,omitempty"`
	ToolChoice        interface{}    `json:"tool_choice,omitempty"`
	ParallelToolCalls bool          `json:"parallel_tool_calls,omitempty"`
	Provider          *ProviderSpec  `json:"provider,omitempty"` // OpenRouter provider specification
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
		Index   int           `json:"index"`
		Message OpenAIMessage `json:"message"`
		FinishReason string   `json:"finish_reason"`
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
func NewOpenAIClient(apiKey, baseURL, model string, temperature float32, systemPrompt string) (*OpenAIClient, error) {
	// API key is optional for some providers (e.g., local Ollama)
	if baseURL == "" {
		return nil, fmt.Errorf("base URL is required")
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

// Name returns the provider name
func (c *OpenAIClient) Name() string {
	// Return a simple name based on the model being used
	return "OpenAI API"
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
		Provider:    c.providerSpec,
	}

	if c.debugTiming {
		log.Debug("AI timing - request preparation", "duration", time.Since(totalStart))
		log.Debug("AI timing - prompt size", "chars", len(prompt))
	}

	// Make API call
	apiCallStart := time.Now()
	resp, err := c.makeRequest(ctx, req)
	if err != nil {
		return "", err
	}

	if c.debugTiming {
		log.Debug("AI timing - API call", "duration", time.Since(apiCallStart))
		log.Debug("AI timing - total", "duration", time.Since(totalStart))
		if resp.Usage.TotalTokens > 0 {
			log.Debug("AI timing - tokens", 
				"total", resp.Usage.TotalTokens,
				"prompt", resp.Usage.PromptTokens,
				"completion", resp.Usage.CompletionTokens)
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
			Provider:    c.providerSpec,
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
		log.Debug("AI timing - model check", "duration", time.Since(checkStart))
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
	
	// Debug: Log full request JSON for troubleshooting
	log.Trace("request JSON", "json", string(jsonData))
	
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
	var providerLogged bool
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
		
		// Log provider info only once (OpenRouter)
		if chunk.Provider != "" && !providerLogged {
			log.Debug("OpenRouter provider", "provider", chunk.Provider)
			providerLogged = true
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

// GenerateWithTools sends a prompt with tool definitions and handles tool calls
func (c *OpenAIClient) GenerateWithTools(ctx context.Context, prompt string, tools []Tool, executor ToolExecutor) (string, error) {
	overallStart := time.Now()
	var toolExecutionTime time.Duration
	var apiCallTime time.Duration
	var toolCallCount int

	// Build initial messages
	systemPrompt := c.systemPrompt
	if systemPrompt == "" {
		systemPrompt = DefaultConfig().SystemPrompt
	}
	
	messages := []OpenAIMessage{
		{
			Role:    "system",
			Content: systemPrompt,
		},
		{
			Role:    "user",
			Content: prompt,
		},
	}

	// Maximum rounds of tool calls to prevent infinite loops
	const maxRounds = 10
	
	for round := 0; round < maxRounds; round++ {
		if round > 0 {
			log.Debug("tool usage round", "round", round+1, "max_rounds", maxRounds)
		}
		
		// Debug: Log message sequence before API call
		log.Debug("message sequence before API call", "round", round+1)
		for i, msg := range messages {
			log.Debug("message", "index", i, "role", msg.Role, "has_tool_calls", len(msg.ToolCalls) > 0, "tool_call_id", msg.ToolCallID)
		}
		
		// Build request with tools
		req := OpenAIRequest{
			Model:             c.model,
			Messages:          messages,
			Temperature:       c.temperature,
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
		log.Debug("model response", 
			"role", responseMsg.Role,
			"content_length", len(responseMsg.Content),
			"tool_calls", len(responseMsg.ToolCalls))
		if round >= 5 && len(responseMsg.ToolCalls) > 0 {
			log.Warn("many tool calls made - model may be stuck", "round", round+1)
		}

		// Check if we have tool calls
		if len(responseMsg.ToolCalls) == 0 {
			// No tool calls, return the content
			totalTime := time.Since(overallStart)
			log.Debug("tool usage completed", "rounds", round+1)
			log.Debug("performance metrics",
				"total_time", totalTime,
				"api_time", apiCallTime,
				"api_time_percent", fmt.Sprintf("%.1f%%", float64(apiCallTime)/float64(totalTime)*100),
				"tool_execution_time", toolExecutionTime,
				"tool_execution_percent", fmt.Sprintf("%.1f%%", float64(toolExecutionTime)/float64(totalTime)*100),
				"tool_call_count", toolCallCount)
			if toolCallCount > 0 {
				log.Debug("tool performance", "avg_time_per_tool", toolExecutionTime/time.Duration(toolCallCount))
			}
			log.Debug("final response", "content_length", len(responseMsg.Content))
			if len(responseMsg.Content) < 2000 {
				log.Trace("final response content", "content", responseMsg.Content)
			}
			return responseMsg.Content, nil
		}

		// Execute tool calls in parallel
		toolResults := c.executeToolsParallel(ctx, responseMsg.ToolCalls, executor, &toolExecutionTime, &toolCallCount)
		
		// Add all tool responses to messages
		messages = append(messages, toolResults...)
		
		// Debug: Log message sequence for Mistral debugging
		log.Debug("message sequence after tool execution")
		for i, msg := range messages {
			log.Debug("message", "index", i, "role", msg.Role, "has_tool_calls", len(msg.ToolCalls) > 0, "tool_call_id", msg.ToolCallID)
		}
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
			log.Trace("tool call arguments raw", "args", string(tc.Function.Arguments))
			
			// Check if Arguments is already a string (double-encoded)
			var argStr string
			if err := json.Unmarshal(tc.Function.Arguments, &argStr); err == nil {
				// It was double-encoded, use the decoded string
				log.Trace("tool call arguments double-encoded", "decoded", argStr)
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
			log.Debug("executing tool", "name", tc.Function.Name, "params", fmt.Sprintf("%v", params))
			
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
			
			log.Trace("tool result", "name", tc.Function.Name, "result", string(resultJSON))

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
		log.Debug("tool timing", "index", result.index, "duration", result.duration)
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