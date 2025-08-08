package llm

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/rail44/mantra/internal/log"
)

// executeToolsParallel executes multiple tool calls in parallel
func (c *OpenAIClient) executeToolsParallel(ctx context.Context, toolCalls []ToolCall, executor ToolExecutor, toolExecutionTime *time.Duration, toolCallCount *int) ([]OpenAIMessage, bool) {
	var wg sync.WaitGroup
	toolResults := make([]OpenAIMessage, len(toolCalls))
	resultToolCalled := false
	mu := &sync.Mutex{}

	for i, toolCall := range toolCalls {
		if toolCall.Type != "function" {
			c.logger.Warn("unsupported tool type", "type", toolCall.Type)
			continue
		}

		wg.Add(1)
		go func(index int, tc ToolCall) {
			defer wg.Done()

			// Parse arguments
			var params map[string]interface{}
			if err := json.Unmarshal([]byte(tc.Function.Arguments), &params); err != nil {
				errorMsg := fmt.Sprintf("failed to parse tool arguments: %v", err)
				c.logger.Error(errorMsg)
				toolResults[index] = OpenAIMessage{
					Role:       "tool",
					Content:    errorMsg,
					ToolCallID: tc.ID,
				}
				return
			}

			// Debug: Log tool call
			c.logger.Debug(fmt.Sprintf("[TOOL] Calling %s", tc.Function.Name))
			if log.IsTraceEnabled() {
				c.logger.Trace(fmt.Sprintf("[TOOL_ARGS] %s: %s", tc.Function.Name, tc.Function.Arguments))
			}

			// Execute tool with timing
			toolStart := time.Now()
			result, err := executor.Execute(ctx, tc.Function.Name, params)
			elapsed := time.Since(toolStart)

			// Track execution time
			mu.Lock()
			*toolExecutionTime += elapsed
			*toolCallCount++
			mu.Unlock()

			// Convert result to JSON string
			var resultContent string
			if err != nil {
				// For errors, create a structured error response
				errorResponse := map[string]interface{}{
					"error": map[string]interface{}{
						"message": err.Error(),
						"type":    "tool_error",
					},
				}
				if resultBytes, marshalErr := json.Marshal(errorResponse); marshalErr == nil {
					resultContent = string(resultBytes)
				} else {
					resultContent = fmt.Sprintf(`{"error": {"message": %q, "type": "tool_error"}}`, err.Error())
				}
				c.logger.Error(fmt.Sprintf("[TOOL] Error from %s: %v", tc.Function.Name, err))
			} else {
				// For success, marshal the result directly
				if result == nil {
					resultContent = "null"
				} else if str, ok := result.(string); ok {
					// If result is already a string, use it directly
					resultContent = str
				} else {
					// Otherwise, convert to JSON
					if resultBytes, err := json.Marshal(result); err == nil {
						resultContent = string(resultBytes)
					} else {
						resultContent = fmt.Sprintf(`{"error": {"message": "failed to marshal result: %v", "type": "marshal_error"}}`, err)
					}
				}

				// Log successful tool execution
				c.logger.Info(fmt.Sprintf("[TOOL] %s completed in %v", tc.Function.Name, elapsed))
				if log.IsTraceEnabled() && len(resultContent) > 0 {
					// Truncate very long results in trace log
					preview := resultContent
					if len(preview) > 500 {
						preview = preview[:500] + "..."
					}
					c.logger.Trace(fmt.Sprintf("[TOOL_RESULT] %s: %s", tc.Function.Name, preview))
				}
			}

			// Check if this is a terminal tool
			if executor.IsTerminal(tc.Function.Name) {
				mu.Lock()
				resultToolCalled = true
				mu.Unlock()
				c.logger.Debug(fmt.Sprintf("Terminal tool '%s' was called", tc.Function.Name))
			}

			toolResults[index] = OpenAIMessage{
				Role:       "tool",
				Content:    resultContent,
				ToolCallID: tc.ID,
			}
		}(i, toolCall)
	}

	wg.Wait()

	// Filter out any empty results (from unsupported tool types)
	var validResults []OpenAIMessage
	for _, result := range toolResults {
		if result.ToolCallID != "" {
			validResults = append(validResults, result)
		}
	}

	// Debug: Log tool results summary
	if len(validResults) > 0 {
		var toolNames []string
		for _, tc := range toolCalls {
			if tc.Type == "function" {
				toolNames = append(toolNames, tc.Function.Name)
			}
		}
		c.logger.Debug(fmt.Sprintf("[TOOLS] Executed %d tools: %s", len(validResults), strings.Join(toolNames, ", ")))
	}

	return validResults, resultToolCalled
}
