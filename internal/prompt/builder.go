package prompt

import (
	"fmt"
	"strings"

	"github.com/rail44/glyph/internal/context"
	"github.com/rail44/glyph/internal/parser"
)

type Builder struct {
}

func NewBuilder() *Builder {
	return &Builder{}
}

// BuildForTarget creates a prompt for a specific generation target
func (b *Builder) BuildForTarget(target *parser.Target, fileContent string) string {
	var prompt strings.Builder

	// Extract relevant context
	ctx, err := context.ExtractRelevantContext(fileContent, target)
	if err != nil {
		// Fallback to basic prompt if context extraction fails
		return b.buildBasicPrompt(target)
	}

	// Build the prompt with rich context
	prompt.WriteString("## Task\n")
	prompt.WriteString(fmt.Sprintf("Implement the body of this Go function: `%s`\n\n", target.GetFunctionSignature()))
	prompt.WriteString(fmt.Sprintf("Instruction: %s\n\n", target.Instruction))

	// Add relevant type definitions
	if len(ctx.Types) > 0 {
		prompt.WriteString("## Relevant Types\n")
		prompt.WriteString("```go\n")
		for _, typeDef := range ctx.Types {
			prompt.WriteString(typeDef)
			prompt.WriteString("\n\n")
		}
		prompt.WriteString("```\n\n")
	}

	// Add imports if they might be needed
	if len(ctx.Imports) > 0 {
		prompt.WriteString("## Available Imports\n")
		prompt.WriteString("```go\n")
		for _, imp := range ctx.Imports {
			prompt.WriteString(fmt.Sprintf("import %s\n", imp))
		}
		prompt.WriteString("```\n\n")
	}

	// Add relevant constants
	if len(ctx.Constants) > 0 {
		prompt.WriteString("## Constants\n")
		prompt.WriteString("```go\n")
		for _, constDef := range ctx.Constants {
			prompt.WriteString(constDef)
			prompt.WriteString("\n")
		}
		prompt.WriteString("```\n\n")
	}

	// Add key instructions
	prompt.WriteString("## Requirements\n")
	prompt.WriteString("- Generate ONLY the code that goes INSIDE the function braces\n")
	prompt.WriteString("- Do NOT include the function signature or braces\n")
	prompt.WriteString("- Start directly with the implementation code\n")
	prompt.WriteString("- Implement the complete specification from the instruction\n")
	prompt.WriteString("- Handle all edge cases mentioned\n")
	prompt.WriteString("- Use the available types and imports as needed\n\n")

	prompt.WriteString("Generate the function body code (what goes inside the braces):\n")

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

// buildBasicPrompt creates a simple prompt when context extraction fails
func (b *Builder) buildBasicPrompt(target *parser.Target) string {
	var prompt strings.Builder
	prompt.WriteString(fmt.Sprintf("Implement this Go function: %s\n", target.GetFunctionSignature()))
	prompt.WriteString(fmt.Sprintf("Task: %s\n", target.Instruction))
	prompt.WriteString("Return only the function body code (the code inside the braces).\n")
	return prompt.String()
}
