package ai

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/rail44/mantra/internal/tools"
	"log/slog"
	"github.com/rail44/mantra/internal/log"
)

// ToolClient wraps an AI client with tool execution capabilities
type ToolClient struct {
	aiClient  *Client
	executor  *tools.Executor
}

// NewToolClient creates a new tool-enabled AI client
func NewToolClient(aiClient *Client, executor *tools.Executor) *ToolClient {
	return &ToolClient{
		aiClient: aiClient,
		executor: executor,
	}
}

// GenerateWithTools handles a conversation with tool support
func (tc *ToolClient) GenerateWithTools(ctx context.Context, prompt string, availableTools []tools.ToolDefinition) (string, error) {
	// Check if provider supports tools
	toolProvider, ok := tc.aiClient.provider.(ToolProvider)
	if !ok {
		// Fallback to regular generation
		log.Warn("provider does not support tools, falling back to regular generation")
		return tc.aiClient.Generate(ctx, prompt)
	}

	// Convert tool definitions to OpenAI format
	openAITools := make([]Tool, len(availableTools))
	for i, tool := range availableTools {
		openAITools[i] = Tool{
			Type: "function",
			Function: ToolFunction{
				Name:        tool.Name,
				Description: tool.Description,
				Parameters:  tool.Parameters,
			},
		}
	}

	// Initialize conversation with system prompt and user message
	messages := []ChatMessage{
		{
			Role:    "system",
			Content: tc.aiClient.config.SystemPrompt,
		},
		{
			Role:    "user",
			Content: prompt,
		},
	}

	// Tool calling loop (max 10 iterations to prevent infinite loops)
	for i := 0; i < 10; i++ {
		// Send messages to AI
		messages, err := toolProvider.GenerateWithTools(ctx, messages, openAITools)
		if err != nil {
			return "", fmt.Errorf("tool generation failed: %w", err)
		}

		// Get the last message
		lastMsg := messages[len(messages)-1]

		// If no tool calls, return the content
		if len(lastMsg.ToolCalls) == 0 {
			return lastMsg.Content, nil
		}

		// Execute tool calls
		for _, toolCall := range lastMsg.ToolCalls {
			log.Debug("executing tool call",
				slog.String("tool", toolCall.Function.Name),
				slog.String("id", toolCall.ID))

			// Parse arguments
			var args map[string]interface{}
			if err := json.Unmarshal(toolCall.Function.Arguments, &args); err != nil {
				// Add error message
				messages = append(messages, ChatMessage{
					Role:       "tool",
					Content:    fmt.Sprintf("Error parsing arguments: %v", err),
					ToolCallID: toolCall.ID,
				})
				continue
			}

			// Execute the tool
			result, err := tc.executor.Execute(ctx, toolCall.Function.Name, args)
			if err != nil {
				// Add error message
				messages = append(messages, ChatMessage{
					Role:       "tool",
					Content:    fmt.Sprintf("Error executing tool: %v", err),
					ToolCallID: toolCall.ID,
				})
				continue
			}

			// Convert result to JSON
			resultJSON, err := json.Marshal(result)
			if err != nil {
				// Add error message
				messages = append(messages, ChatMessage{
					Role:       "tool",
					Content:    fmt.Sprintf("Error marshaling result: %v", err),
					ToolCallID: toolCall.ID,
				})
				continue
			}

			// Add tool result to conversation
			messages = append(messages, ChatMessage{
				Role:       "tool",
				Content:    string(resultJSON),
				ToolCallID: toolCall.ID,
			})
		}
	}

	return "", fmt.Errorf("exceeded maximum tool calling iterations")
}

// GenerateStreamWithTools handles streaming generation with tool support
// Note: This is more complex and might not be fully supported by all providers
func (tc *ToolClient) GenerateStreamWithTools(ctx context.Context, prompt string, availableTools []tools.ToolDefinition) (<-chan string, <-chan error) {
	outputCh := make(chan string, 100)
	errorCh := make(chan error, 1)

	// For now, use non-streaming version and send result at once
	go func() {
		defer close(outputCh)
		defer close(errorCh)

		result, err := tc.GenerateWithTools(ctx, prompt, availableTools)
		if err != nil {
			errorCh <- err
			return
		}

		outputCh <- result
	}()

	return outputCh, errorCh
}