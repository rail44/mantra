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
}

// ToolDefinition represents the metadata about a tool
type ToolDefinition struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	Parameters  json.RawMessage `json:"parameters"`
}