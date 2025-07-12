package prompt

import (
	"fmt"
	"strings"

	"github.com/rail44/glyph/internal/parser"
)

type Builder struct {
	context *Context
}

func NewBuilder(context *Context) *Builder {
	return &Builder{
		context: context,
	}
}

// Build creates a comprehensive prompt for the AI
func (b *Builder) Build(decl *parser.Declaration) string {
	var prompt strings.Builder

	prompt.WriteString("Generate a Go implementation for a Spanner data access layer based on the following declaration.\n\n")

	// Add declaration context
	prompt.WriteString("## Declaration\n")
	prompt.WriteString("```go\n")
	prompt.WriteString(b.context.Declaration)
	prompt.WriteString("\n```\n\n")

	// Add parsed structure information
	prompt.WriteString("## Structure Analysis\n")
	prompt.WriteString(fmt.Sprintf("- Request Type: %s\n", decl.RequestType))
	prompt.WriteString(fmt.Sprintf("- Response Type: %s\n", decl.ResponseType))
	if decl.Description != "" {
		prompt.WriteString(fmt.Sprintf("- Purpose: %s\n", decl.Description))
	}
	prompt.WriteString("\n")

	// Add field information
	if len(decl.Fields) > 0 {
		prompt.WriteString("### Request Fields:\n")
		for _, field := range decl.Fields {
			prompt.WriteString(fmt.Sprintf("- %s (%s)", field.Name, field.Type))
			if field.Comment != "" {
				prompt.WriteString(fmt.Sprintf(" - %s", field.Comment))
			}
			prompt.WriteString("\n")
		}
		prompt.WriteString("\n")
	}

	// Add existing implementation context if available
	if b.context.ExistingImpl != "" {
		prompt.WriteString("## Existing Implementation\n")
		prompt.WriteString("An implementation already exists. Please update it based on the new declaration while preserving any custom logic or optimizations.\n")
		prompt.WriteString("```go\n")
		prompt.WriteString(b.context.ExistingImpl)
		prompt.WriteString("\n```\n\n")
	}

	// Add human edits if detected
	if b.context.HumanEdits != "" {
		prompt.WriteString("## Important: Preserve Human Modifications\n")
		prompt.WriteString("The following sections contain manual modifications that should be preserved:\n")
		prompt.WriteString(b.context.HumanEdits)
		prompt.WriteString("\n\n")
	}

	// Add Spanner best practices
	prompt.WriteString("## Guidelines\n")
	prompt.WriteString(b.context.SpannerKnowledge)
	prompt.WriteString("\n")

	// Add generation instructions
	prompt.WriteString("## Requirements\n")
	prompt.WriteString("1. Generate a complete implementation file\n")
	prompt.WriteString("2. Include all necessary imports\n")
	prompt.WriteString("3. Implement a function that accepts context.Context and *spanner.Client\n")
	prompt.WriteString("4. Use proper error handling with wrapped errors\n")
	prompt.WriteString("5. Follow Go idioms and best practices\n")
	prompt.WriteString("6. Optimize queries for Spanner's distributed architecture\n")
	prompt.WriteString("7. Include helpful comments explaining complex logic\n")
	prompt.WriteString("\n")

	prompt.WriteString("Generate only the Go code without any explanation or markdown formatting.\n")

	return prompt.String()
}