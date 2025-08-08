package ai

import "context"

// Provider represents an AI service provider
type Provider interface {
	// Generate sends a prompt with tool definitions and handles tool calls
	Generate(ctx context.Context, prompt string, tools []Tool, executor ToolExecutor) (string, error)

	// Name returns the provider name
	Name() string

	// SetTemperature sets the temperature for generation
	SetTemperature(temperature float32)

	// SetSystemPrompt sets the system prompt
	SetSystemPrompt(systemPrompt string)

	// SetResponseFormat sets the structured output format (optional)
	SetResponseFormat(format *ResponseFormat)
}

// ToolExecutor executes tool calls
type ToolExecutor interface {
	Execute(ctx context.Context, toolName string, params map[string]interface{}) (interface{}, error)
}
