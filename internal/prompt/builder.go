package prompt

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/rail44/mantra/internal/context"
	"github.com/rail44/mantra/internal/parser"
)

// Builder creates prompts for code generation
type Builder struct{}

// NewBuilder creates a new prompt builder
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

	// Add examples from previously generated file
	extractor := context.NewExampleExtractor()
	
	// Look for generated file
	generatedDir := "generated" // TODO: This should be configurable
	generatedPath := filepath.Join(generatedDir, filepath.Base(target.FilePath))
	
	if generatedContent, err := os.ReadFile(generatedPath); err == nil {
		examples, _ := extractor.ExtractFromFileContent(string(generatedContent), target)
		if len(examples) > 0 {
			prompt.WriteString("## Examples from previously generated implementations\n")
			prompt.WriteString("Here are some functions that were previously generated for this file:\n\n")
			
			// Limit to 2 examples to keep prompt size reasonable
			maxExamples := 2
			if len(examples) < maxExamples {
				maxExamples = len(examples)
			}
			
			for i := 0; i < maxExamples; i++ {
				example := examples[i]
				prompt.WriteString(fmt.Sprintf("### Example: %s\n", example.Signature))
				prompt.WriteString("```go\n")
				prompt.WriteString(example.Body)
				prompt.WriteString("\n```\n\n")
			}
		}
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

// buildBasicPrompt creates a simple prompt when context extraction fails
func (b *Builder) buildBasicPrompt(target *parser.Target) string {
	var prompt strings.Builder
	prompt.WriteString(fmt.Sprintf("Implement this Go function: %s\n", target.GetFunctionSignature()))
	prompt.WriteString(fmt.Sprintf("Task: %s\n", target.Instruction))
	prompt.WriteString("Return only the function body code (the code inside the braces).\n")
	return prompt.String()
}
