package ai

import "context"

// Provider represents an AI service provider
type Provider interface {
	// Generate sends a prompt with tool definitions and handles tool calls
	Generate(ctx context.Context, prompt string, tools []Tool, executor ToolExecutor) (string, error)
	
	// CheckModel verifies if the specified model is available
	CheckModel(ctx context.Context) error
	
	// Name returns the provider name
	Name() string
}

// ToolExecutor executes tool calls
type ToolExecutor interface {
	Execute(ctx context.Context, toolName string, params map[string]interface{}) (interface{}, error)
}