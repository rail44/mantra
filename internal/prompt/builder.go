package prompt

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/rail44/glyph/internal/parser"
)

type Builder struct {
	mode string
}

func NewBuilder(mode string) *Builder {
	return &Builder{
		mode: mode,
	}
}

// BuildForTarget creates a prompt for a specific generation target
func (b *Builder) BuildForTarget(target *parser.Target, fileContent string) string {
	var prompt strings.Builder

	// For simple functions, use a minimal prompt
	if b.isSimpleFunction(target) {
		prompt.WriteString(fmt.Sprintf("Implement this Go function: %s\n", target.GetFunctionSignature()))
		prompt.WriteString(fmt.Sprintf("Task: %s\n", target.Instruction))
		prompt.WriteString("Return only the function body code without the signature.\n")
		return prompt.String()
	}

	// For complex functions, use detailed prompt
	// Add context about the task
	prompt.WriteString("Generate a Go implementation based on the following:\n\n")

	// Add file context
	prompt.WriteString("## Source File\n")
	prompt.WriteString(fmt.Sprintf("File: %s\n", filepath.Base(target.FilePath)))
	prompt.WriteString(fmt.Sprintf("Function: %s\n\n", target.GetFunctionSignature()))

	// Add the natural language instruction
	prompt.WriteString("## Task Description\n")
	prompt.WriteString(target.Instruction)
	prompt.WriteString("\n\n")

	// Add function signature details
	prompt.WriteString("## Function Details\n")
	prompt.WriteString(fmt.Sprintf("- Name: %s\n", target.Name))
	
	if target.Receiver != nil {
		prompt.WriteString(fmt.Sprintf("- Method of: %s\n", target.Receiver.Type))
	}
	
	if len(target.Params) > 0 {
		prompt.WriteString("- Parameters:\n")
		for _, param := range target.Params {
			if param.Name != "" {
				prompt.WriteString(fmt.Sprintf("  - %s: %s\n", param.Name, param.Type))
			} else {
				prompt.WriteString(fmt.Sprintf("  - %s\n", param.Type))
			}
		}
	}
	
	if len(target.Returns) > 0 {
		prompt.WriteString("- Returns:\n")
		for _, ret := range target.Returns {
			prompt.WriteString(fmt.Sprintf("  - %s\n", ret.Type))
		}
	}
	
	prompt.WriteString("\n")

	// Add mode-specific context
	if b.mode == "spanner" {
		prompt.WriteString("## Spanner Context\n")
		prompt.WriteString("This function should be optimized for Google Cloud Spanner:\n")
		prompt.WriteString("- Use parameterized queries to prevent SQL injection\n")
		prompt.WriteString("- Consider read-only transactions for queries\n")
		prompt.WriteString("- Use appropriate indexes for performance\n")
		prompt.WriteString("- Handle Spanner-specific errors appropriately\n")
		prompt.WriteString("\n")
	}

	// Add relevant code context from the file
	prompt.WriteString("## File Context\n")
	prompt.WriteString("```go\n")
	// Include first 100 lines or up to function start for context
	lines := strings.Split(fileContent, "\n")
	contextEnd := min(100, target.StartLine-1)
	if contextEnd > 0 {
		prompt.WriteString(strings.Join(lines[:contextEnd], "\n"))
	}
	prompt.WriteString("\n```\n\n")

	// Add generation instructions
	prompt.WriteString("## Instructions\n")
	prompt.WriteString("1. Generate ONLY the function body (the code that goes inside the function)\n")
	prompt.WriteString("2. Do NOT include the function signature\n")
	prompt.WriteString("3. Do NOT include package declaration or imports\n")
	prompt.WriteString("4. Replace the panic(\"not implemented\") with actual implementation\n")
	prompt.WriteString("5. Use proper error handling\n")
	prompt.WriteString("6. Follow Go idioms and best practices\n")
	prompt.WriteString("7. Ensure the implementation matches the task description\n")
	prompt.WriteString("\n")

	prompt.WriteString("Generate only the function body implementation:\n")

	return prompt.String()
}

// isSimpleFunction determines if a function is simple enough for a minimal prompt
func (b *Builder) isSimpleFunction(target *parser.Target) bool {
	// Simple criteria:
	// - No receiver (not a method)
	// - Less than 3 parameters
	// - Less than 3 return values
	// - Short instruction (less than 100 chars)
	return target.Receiver == nil &&
		len(target.Params) < 3 &&
		len(target.Returns) < 3 &&
		len(target.Instruction) < 100
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}