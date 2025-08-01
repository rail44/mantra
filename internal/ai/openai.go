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
	Content    string     `json:"content,omitempty"`
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
		Provider:    c.providerSpec,
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

	// For OpenAI-compatible APIs, we'll do a simple completion request
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
	
	// Debug: Log request with provider info
	if c.providerSpec != nil {
		fmt.Printf("[DEBUG] Sending request with provider spec: %+v\n", c.providerSpec)
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
		fmt.Printf("[PROVIDER] OpenRouter used provider: %s\n", result.Provider)
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
		
		// Log provider info if available (OpenRouter)
		if chunk.Provider != "" {
			fmt.Printf("[PROVIDER] OpenRouter used provider: %s\n", chunk.Provider)
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
			fmt.Printf("[TOOL USAGE] Starting round %d/%d\n", round+1, maxRounds)
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
		
		messages = append(messages, responseMsg)

		// Debug: Log response
		fmt.Printf("[DEBUG] Response from model: role=%s, content=%d chars, tool_calls=%d\n", 
			responseMsg.Role, len(responseMsg.Content), len(responseMsg.ToolCalls))
		if round >= 5 && len(responseMsg.ToolCalls) > 0 {
			fmt.Printf("[WARNING] Many tool calls made (round %d) - model may be stuck\n", round+1)
		}

		// Check if we have tool calls
		if len(responseMsg.ToolCalls) == 0 {
			// No tool calls, return the content
			totalTime := time.Since(overallStart)
			fmt.Printf("[TOOL USAGE] Completed successfully after %d round(s)\n", round+1)
			fmt.Printf("[PERFORMANCE] Total time: %v\n", totalTime)
			fmt.Printf("[PERFORMANCE] API calls time: %v (%.1f%%)\n", apiCallTime, float64(apiCallTime)/float64(totalTime)*100)
			fmt.Printf("[PERFORMANCE] Tool execution time: %v (%.1f%%)\n", toolExecutionTime, float64(toolExecutionTime)/float64(totalTime)*100)
			fmt.Printf("[PERFORMANCE] Tool calls: %d\n", toolCallCount)
			if toolCallCount > 0 {
				fmt.Printf("[PERFORMANCE] Avg time per tool: %v\n", toolExecutionTime/time.Duration(toolCallCount))
			}
			fmt.Printf("[FINAL RESPONSE] Content length: %d chars\n", len(responseMsg.Content))
			if len(responseMsg.Content) < 2000 {
				fmt.Printf("[FINAL RESPONSE] Content: %q\n", responseMsg.Content)
			}
			return responseMsg.Content, nil
		}

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
			fmt.Printf("[TOOL CALL DEBUG] Arguments raw: %q\n", string(tc.Function.Arguments))
			
			// Check if Arguments is already a string (double-encoded)
			var argStr string
			if err := json.Unmarshal(tc.Function.Arguments, &argStr); err == nil {
				// It was double-encoded, use the decoded string
				fmt.Printf("[TOOL CALL DEBUG] Arguments was double-encoded, decoded to: %s\n", argStr)
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
			fmt.Printf("[TOOL CALL] Executing %s with params: %v\n", tc.Function.Name, params)
			
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
			
			fmt.Printf("[TOOL RESULT] %s: %s\n", tc.Function.Name, string(resultJSON))

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
		fmt.Printf("[TOOL TIMING] Tool at index %d took %v\n", result.index, result.duration)
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