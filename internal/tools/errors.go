package tools

// ToolError represents an error from tool execution
type ToolError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
	Details string `json:"details,omitempty"`
}

func (e *ToolError) Error() string {
	if e.Details != "" {
		return e.Message + ": " + e.Details
	}
	return e.Message
}

// NewToolError creates a new tool error
func NewToolError(code, message string) *ToolError {
	return &ToolError{
		Code:    code,
		Message: message,
	}
}

// NewToolErrorWithDetails creates a new tool error with details
func NewToolErrorWithDetails(code, message, details string) *ToolError {
	return &ToolError{
		Code:    code,
		Message: message,
		Details: details,
	}
}