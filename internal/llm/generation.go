package llm

import (
	"context"
	"fmt"
	"time"

	"log/slog"
)

// Generate sends a prompt with tool definitions and handles tool calls
func (c *OpenAIClient) Generate(ctx context.Context, prompt string, tools []Tool, executor ToolExecutor) (string, error) {
	overallStart := time.Now()
	var toolExecutionTime time.Duration
	var apiCallTime time.Duration
	var toolCallCount int

	// Use the logger directly
	logger := c.logger

	// Log system prompt at debug level
	if c.systemPrompt != "" {
		logger.Debug("System prompt set", "length", len(c.systemPrompt))
		// Log full system prompt at debug level
		logger.Debug(fmt.Sprintf("[SYSTEM_PROMPT]\n%s", c.systemPrompt))
	}

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

	// Track if result tool has been called
	resultToolCalled := false

	for round := 0; round < maxRounds; round++ {
		if round > 0 {
			logger.Debug("tool usage round", "round", round+1, "max_rounds", maxRounds)
		}

		// Log concise message stats
		logger.Debug(fmt.Sprintf("[ROUND] %d/%d: %d messages", round+1, maxRounds, len(messages)))

		// Use the current temperature set by the phase
		temperature := c.currentTemperature

		req := OpenAIRequest{
			Model:             c.model,
			Messages:          messages,
			Temperature:       temperature,
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
			Reasoning: responseMsg.Reasoning, // Preserve reasoning for models that support it
			ToolCalls: responseMsg.ToolCalls,
		}
		messages = append(messages, cleanMsg)

		// Debug: Log response
		logger.Debug(fmt.Sprintf("[API] Response: %s (content=%d chars, tools=%d)",
			responseMsg.Role, len(responseMsg.Content), len(responseMsg.ToolCalls)))
		if round >= 5 && len(responseMsg.ToolCalls) > 0 {
			logger.Warn("many tool calls made - model may be stuck", "round", round+1)
		}

		// Check if we have tool calls
		if len(responseMsg.ToolCalls) == 0 {
			// Check if there's a result tool available but not used
			hasResultTool := false
			for _, tool := range tools {
				if tool.Function.Name == "result" {
					hasResultTool = true
					break
				}
			}

			// If result tool exists but wasn't called yet, prompt the AI to use it
			if hasResultTool && !resultToolCalled && round < maxRounds-1 { // Leave one round for the final attempt
				logger.Debug("No tool calls made but result tool is available, prompting to use it")
				messages = append(messages, OpenAIMessage{
					Role:    "user",
					Content: "Please complete the task by calling the result() tool with the appropriate data. The result() tool is required to finalize this phase.",
				})
				continue
			}

			// No tool calls and no result tool, or result tool already called - return the content
			if responseMsg.Content != "" {
				logger.Debug("Returning final response")
				c.logTimingStats(logger, overallStart, apiCallTime, toolExecutionTime, toolCallCount)
				return responseMsg.Content, nil
			}

			// No content and no tool calls - this is unusual
			logger.Warn("No content and no tool calls in response")
			c.logTimingStats(logger, overallStart, apiCallTime, toolExecutionTime, toolCallCount)
			return "", fmt.Errorf("model returned empty response without tool calls")
		}

		// Execute all tool calls in parallel
		toolResults, wasResultCalled := c.executeToolsParallel(ctx, responseMsg.ToolCalls, executor, &toolExecutionTime, &toolCallCount, logger)
		if wasResultCalled {
			resultToolCalled = true
		}

		// Add all tool results to messages
		messages = append(messages, toolResults...)

		// Check if any tool is terminal
		for _, toolCall := range responseMsg.ToolCalls {
			if toolCall.Type == "function" && executor.IsTerminal(toolCall.Function.Name) {
				logger.Debug(fmt.Sprintf("Terminal tool '%s' executed, ending conversation", toolCall.Function.Name))

				// Find and return the result from the terminal tool
				for _, result := range toolResults {
					if result.ToolCallID == toolCall.ID {
						c.logTimingStats(logger, overallStart, apiCallTime, toolExecutionTime, toolCallCount)
						return result.Content, nil
					}
				}
			}
		}
	}

	logger.Warn("Reached maximum rounds of tool calls", "max_rounds", maxRounds)
	c.logTimingStats(logger, overallStart, apiCallTime, toolExecutionTime, toolCallCount)
	return "", fmt.Errorf("exceeded maximum rounds (%d) of tool calls", maxRounds)
}

// logTimingStats logs timing statistics for the generation
func (c *OpenAIClient) logTimingStats(logger *slog.Logger, overallStart time.Time, apiCallTime, toolExecutionTime time.Duration, toolCallCount int) {
	totalTime := time.Since(overallStart)
	logger.Info(fmt.Sprintf("[TIMING] Total: %v, API: %v, Tools: %v (%d calls)",
		totalTime.Round(time.Millisecond),
		apiCallTime.Round(time.Millisecond),
		toolExecutionTime.Round(time.Millisecond),
		toolCallCount))
}
