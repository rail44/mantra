package llm

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"sort"
	"sync"
	"time"

	"golang.org/x/sync/errgroup"
)

// toolResult represents the result of a single tool execution
type toolResult struct {
	index      int
	toolCallID string
	message    OpenAIMessage
	duration   time.Duration
	isTerminal bool
}

// executeToolsParallel executes multiple tool calls in parallel using channels for efficient result collection
func (c *OpenAIClient) executeToolsParallel(ctx context.Context, toolCalls []ToolCall, executor ToolExecutor, toolExecutionTime *time.Duration, toolCallCount *int, logger *slog.Logger) ([]OpenAIMessage, bool) {
	results := make(chan toolResult, len(toolCalls))
	resultToolCalled := false
	mu := &sync.Mutex{}

	// Use errgroup with limited concurrency
	g, ctx := errgroup.WithContext(ctx)

	// Execute all tools in parallel
	for i, toolCall := range toolCalls {
		if toolCall.Type != "function" {
			logger.Warn("unsupported tool type", "type", toolCall.Type)
			continue
		}

		// Capture loop variables
		index := i
		tc := toolCall

		g.Go(func() error {

			// Parse arguments
			// Check if Arguments is already a string (double-encoded by some providers like Mistral)
			var argStr string
			if err := json.Unmarshal(tc.Function.Arguments, &argStr); err == nil {
				// It was double-encoded, use the decoded string
				tc.Function.Arguments = json.RawMessage(argStr)
			}

			var params map[string]any
			if err := json.Unmarshal(tc.Function.Arguments, &params); err != nil {
				errorMsg := fmt.Sprintf("failed to parse tool arguments: %v", err)
				logger.Error(errorMsg)
				results <- toolResult{
					index:      index,
					toolCallID: tc.ID,
					message: OpenAIMessage{
						Role:       "tool",
						Content:    errorMsg,
						ToolCallID: tc.ID,
					},
				}
				return nil
			}

			// Execute tool with timing
			toolStart := time.Now()
			result, err := executor.Execute(ctx, tc.Function.Name, params)
			elapsed := time.Since(toolStart)

			// Convert result to JSON string
			var resultContent string
			if err != nil {
				// For errors, create a structured error response
				errorResponse := map[string]any{
					"error": map[string]any{
						"message": err.Error(),
						"type":    "tool_error",
					},
				}
				if resultBytes, marshalErr := json.Marshal(errorResponse); marshalErr == nil {
					resultContent = string(resultBytes)
				} else {
					resultContent = fmt.Sprintf(`{"error": {"message": %q, "type": "tool_error"}}`, err.Error())
				}
				// Log tool error
				logger.Info("Tool error",
					slog.String("tool", tc.Function.Name),
					slog.String("error", err.Error()),
					slog.Duration("duration", elapsed))
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

				// Special handling for result tool to check success/failure
				if tc.Function.Name == "result" && params != nil {
					// Check if the result tool was called with success: false
					if success, ok := params["success"].(bool); ok && !success {
						// Phase failed - use warning log
						logger.Warn("Phase failed via result tool",
							slog.String("tool", tc.Function.Name),
							slog.Duration("duration", elapsed))
					} else {
						// Phase succeeded (or success field not found)
						logger.Info("Tool completed",
							slog.String("tool", tc.Function.Name),
							slog.Duration("duration", elapsed))
					}
				} else {
					// Normal tool execution
					logger.Info("Tool completed",
						slog.String("tool", tc.Function.Name),
						slog.Duration("duration", elapsed))
				}
			}

			// Check if this is a terminal tool
			isTerminal := executor.IsTerminal(tc.Function.Name)

			results <- toolResult{
				index:      index,
				toolCallID: tc.ID,
				duration:   elapsed,
				isTerminal: isTerminal,
				message: OpenAIMessage{
					Role:       "tool",
					Content:    resultContent,
					ToolCallID: tc.ID,
				},
			}

			return nil
		})
	}

	// Wait for all tools to complete and close the channel
	go func() {
		g.Wait()
		close(results)
	}()

	// Collect results from channel
	var resultSlice []toolResult
	for result := range results {
		resultSlice = append(resultSlice, result)

		// Update shared timing stats
		mu.Lock()
		*toolExecutionTime += result.duration
		*toolCallCount++
		if result.isTerminal {
			resultToolCalled = true
		}
		mu.Unlock()
	}

	// Sort results by original index to maintain order
	sort.Slice(resultSlice, func(i, j int) bool {
		return resultSlice[i].index < resultSlice[j].index
	})

	// Extract messages in correct order
	messages := make([]OpenAIMessage, 0, len(resultSlice))
	for _, result := range resultSlice {
		messages = append(messages, result.message)
	}

	return messages, resultToolCalled
}
