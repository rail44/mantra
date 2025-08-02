package impl

import (
	"context"
	"encoding/json"
	"fmt"
	"go/parser"
	"go/scanner"
	"go/token"
	"strings"

	"github.com/rail44/mantra/internal/tools"
)

// CheckSyntaxTool validates Go code syntax
type CheckSyntaxTool struct{}

// NewCheckSyntaxTool creates a new check_syntax tool
func NewCheckSyntaxTool() *CheckSyntaxTool {
	return &CheckSyntaxTool{}
}

// Name returns the tool name
func (t *CheckSyntaxTool) Name() string {
	return "check_syntax"
}

// Description returns what this tool does
func (t *CheckSyntaxTool) Description() string {
	return "Check if Go code is syntactically correct"
}

// ParametersSchema returns the JSON Schema for parameters
func (t *CheckSyntaxTool) ParametersSchema() json.RawMessage {
	return json.RawMessage(`{
		"type": "object",
		"properties": {
			"code": {
				"type": "string",
				"description": "The Go code to validate"
			},
			"context": {
				"type": "string",
				"enum": ["function_body", "complete_function", "expression", "statement"],
				"default": "function_body",
				"description": "The context of the code"
			}
		},
		"required": ["code"]
	}`)
}

// Execute runs the check_syntax tool
func (t *CheckSyntaxTool) Execute(ctx context.Context, params map[string]interface{}) (interface{}, error) {
	// Extract parameters
	code, ok := params["code"].(string)
	if !ok {
		return nil, &tools.ToolError{
			Code:    "invalid_params",
			Message: "Parameter 'code' is required and must be a string",
		}
	}

	context := "function_body"
	if c, ok := params["context"].(string); ok {
		context = c
	}

	// Wrap code based on context
	wrappedCode := t.wrapCode(code, context)

	// Parse the code
	fset := token.NewFileSet()
	_, err := parser.ParseFile(fset, "check.go", wrappedCode, parser.AllErrors)

	if err == nil {
		return CheckSyntaxResult{
			Valid: true,
		}, nil
	}

	// Extract error details
	errors := t.extractErrors(err, code, context, fset)

	return CheckSyntaxResult{
		Valid:  false,
		Errors: errors,
	}, nil
}

// CheckSyntaxResult represents the result of syntax checking
type CheckSyntaxResult struct {
	Valid  bool          `json:"valid"`
	Errors []SyntaxError `json:"errors,omitempty"`
}

// SyntaxError represents a syntax error
type SyntaxError struct {
	Line    int    `json:"line"`
	Column  int    `json:"column"`
	Message string `json:"message"`
	Hint    string `json:"hint,omitempty"`
}

func (t *CheckSyntaxTool) wrapCode(code, context string) string {
	switch context {
	case "function_body":
		// Wrap as a function body
		return fmt.Sprintf(`package p

func _() {
%s
}`, code)

	case "complete_function":
		// Assume it's a complete function, just add package
		return fmt.Sprintf(`package p

%s`, code)

	case "expression":
		// Wrap as a variable assignment
		return fmt.Sprintf(`package p

var _ = %s`, code)

	case "statement":
		// Wrap as a single statement in a function
		return fmt.Sprintf(`package p

func _() {
	%s
}`, code)

	default:
		// Return as-is
		return code
	}
}

func (t *CheckSyntaxTool) extractErrors(err error, originalCode, context string, fset *token.FileSet) []SyntaxError {
	var errors []SyntaxError

	// Handle scanner errors
	if list, ok := err.(scanner.ErrorList); ok {
		for _, e := range list {
			errors = append(errors, t.convertError(e, originalCode, context, fset))
		}
		return errors
	}

	// Handle single error
	if e, ok := err.(*scanner.Error); ok {
		errors = append(errors, t.convertError(e, originalCode, context, fset))
		return errors
	}

	// Generic error
	errors = append(errors, SyntaxError{
		Line:    1,
		Column:  1,
		Message: err.Error(),
	})

	return errors
}

func (t *CheckSyntaxTool) convertError(e *scanner.Error, originalCode, context string, fset *token.FileSet) SyntaxError {
	// Adjust line numbers based on context wrapping
	line := e.Pos.Line
	column := e.Pos.Column

	switch context {
	case "function_body":
		// Subtract the wrapping lines (package + func declaration)
		line -= 3
	case "complete_function":
		// Subtract package line
		line -= 2
	case "expression":
		// Subtract package and var lines
		line -= 2
	case "statement":
		// Subtract package and func lines
		line -= 3
	}

	// Ensure line is at least 1
	if line < 1 {
		line = 1
	}

	syntaxErr := SyntaxError{
		Line:    line,
		Column:  column,
		Message: e.Msg,
	}

	// Add helpful hints based on common errors
	syntaxErr.Hint = t.generateHint(e.Msg, originalCode)

	return syntaxErr
}

func (t *CheckSyntaxTool) generateHint(message, code string) string {
	msg := strings.ToLower(message)

	switch {
	case strings.Contains(msg, "expected ')'"):
		return "Missing closing parenthesis. Check your function calls and expressions."

	case strings.Contains(msg, "expected '}'"):
		return "Missing closing brace. Check your if statements, loops, and function bodies."

	case strings.Contains(msg, "expected ';'"):
		return "Missing semicolon or newline. In Go, statements are usually terminated by newlines."

	case strings.Contains(msg, "unexpected"):
		if strings.Contains(msg, "end of statement") {
			return "Syntax error at end of statement. Check for missing operators or incomplete expressions."
		}
		return "Unexpected token. Check for typos or incorrect syntax."

	case strings.Contains(msg, "missing ',' in"):
		return "Missing comma in list. Check function arguments, slice literals, or composite literals."

	case strings.Contains(msg, "cannot use"):
		return "Type mismatch. Ensure you're using the correct types for operations."

	default:
		// Try to provide context-specific hints
		if strings.Contains(code, "if ") && !strings.Contains(code, "{") {
			return "If statements in Go must use braces, even for single statements."
		}
		if strings.Contains(code, "return") && strings.Contains(message, "not enough arguments") {
			return "Check that your return statement matches the function's return signature."
		}
	}

	return ""
}
