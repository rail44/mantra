package ai

import (
	"encoding/json"
)

// ToolCall represents a request to call a tool (OpenAI format)
type ToolCall struct {
	ID       string           `json:"id"`
	Type     string           `json:"type"`
	Function ToolCallFunction `json:"function"`
}

// ToolCallFunction represents the function to call
type ToolCallFunction struct {
	Name      string          `json:"name"`
	Arguments json.RawMessage `json:"arguments"`
}

// Tool represents a tool definition (OpenAI format)
type Tool struct {
	Type     string       `json:"type"`
	Function ToolFunction `json:"function"`
}

// ToolFunction represents a function tool
type ToolFunction struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	Parameters  json.RawMessage `json:"parameters"`
}

// ChatMessage represents a message in the conversation (extended for tools)
type ChatMessage struct {
	Role       string     `json:"role"`
	Content    string     `json:"content"`
	ToolCalls  []ToolCall `json:"tool_calls,omitempty"`
	ToolCallID string     `json:"tool_call_id,omitempty"`
}

// ToolChoice represents how the model should use tools
type ToolChoice interface{}

// Specific tool choice types
type (
	ToolChoiceAuto     struct{} // "auto" - let the model decide
	ToolChoiceNone     struct{} // "none" - don't use tools
	ToolChoiceRequired struct{} // "required" - must use a tool
	ToolChoiceFunction struct {
		Type     string `json:"type"`
		Function struct {
			Name string `json:"name"`
		} `json:"function"`
	}
)

// MarshalJSON implementations for tool choice types
func (ToolChoiceAuto) MarshalJSON() ([]byte, error) {
	return json.Marshal("auto")
}

func (ToolChoiceNone) MarshalJSON() ([]byte, error) {
	return json.Marshal("none")
}

func (ToolChoiceRequired) MarshalJSON() ([]byte, error) {
	return json.Marshal("required")
}
