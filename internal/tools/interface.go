package tools

import (
	"context"
	"encoding/json"
)

// Tool represents a tool that can be called by the AI
type Tool interface {
	// Name returns the name of the tool
	Name() string

	// Description returns a human-readable description of what the tool does
	Description() string

	// ParametersSchema returns the JSON Schema for the tool's parameters
	ParametersSchema() json.RawMessage

	// Execute runs the tool with the given parameters
	Execute(ctx context.Context, params map[string]interface{}) (interface{}, error)

	// IsTerminal returns true if this tool ends the current phase
	IsTerminal() bool
}

// ContextAwareTool is a tool that can receive additional context from the system
// This is useful for tools that need access to information not provided by the AI
type ContextAwareTool interface {
	Tool

	// SetContext provides the tool with system context
	// This is called before Execute if context is available
	SetContext(toolCtx *Context)
}
