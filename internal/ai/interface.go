package ai

import "context"

// Provider represents an AI service provider
type Provider interface {
	// Generate sends a prompt to the AI and returns the response
	Generate(ctx context.Context, prompt string) (string, error)
	
	// GenerateStream sends a prompt and returns channels for streaming responses
	GenerateStream(ctx context.Context, prompt string) (<-chan string, <-chan error)
	
	// CheckModel verifies if the specified model is available
	CheckModel(ctx context.Context) error
	
	// Name returns the provider name
	Name() string
}

// ToolProvider represents an AI provider that supports tool/function calling
type ToolProvider interface {
	Provider
	
	// GenerateWithTools sends a prompt with tool definitions and handles tool calls
	GenerateWithTools(ctx context.Context, prompt string, tools []Tool, executor ToolExecutor) (string, error)
}

// ToolExecutor executes tool calls
type ToolExecutor interface {
	Execute(ctx context.Context, toolName string, params map[string]interface{}) (interface{}, error)
}