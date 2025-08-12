package prompt

import (
	"fmt"
	"log/slog"
	"strings"

	"github.com/rail44/mantra/internal/context"
	"github.com/rail44/mantra/internal/parser"
)

// Builder creates prompts for code generation
type Builder struct {
	useTools          bool
	additionalContext string
	logger            *slog.Logger
}

// NewBuilder creates a new prompt builder
func NewBuilder(logger *slog.Logger) *Builder {
	if logger == nil {
		// Create a default logger if none provided
		logger = slog.Default()
	}
	return &Builder{
		logger: logger,
	}
}

// WithAdditionalContext sets additional context to be included in the prompt
func (b *Builder) WithAdditionalContext(context string) *Builder {
	b.additionalContext = context
	return b
}

// SetUseTools enables or disables tool usage instructions in prompts
func (b *Builder) SetUseTools(useTools bool) {
	b.useTools = useTools
}

// BuildForTarget creates a prompt for a specific generation target
func (b *Builder) BuildForTarget(target *parser.Target, fileContent string) (string, error) {
	// Use function-focused context extraction for reliable type information
	ctx, err := context.ExtractFunctionContext(target.FilePath, target)
	if err != nil {
		b.logger.Error("context extraction failed", slog.String("error", err.Error()))
		return "", fmt.Errorf("context extraction failed: %w", err)
	}

	return b.buildPromptWithContext(ctx, target), nil
}

// buildPromptWithContext builds a prompt using the extracted context
func (b *Builder) buildPromptWithContext(ctx *context.RelevantContext, target *parser.Target) string {
	var prompt strings.Builder

	// DevStral最適化：XMLタグで構造化
	prompt.WriteString("<context>\n")

	// All imports are treated as available packages for the AI
	if len(ctx.Imports) > 0 {
		prompt.WriteString("Available packages:\n")
		for _, imp := range ctx.Imports {
			identifier := imp.GetIdentifier()

			// For blank imports, we still show them as available packages
			// The AI doesn't need to know about the blank import detail
			if imp.Path == identifier {
				// Standard library or simple package
				prompt.WriteString(fmt.Sprintf("- %s\n", imp.Path))
			} else if imp.Alias != "" && imp.Alias != "_" && imp.Alias != identifier {
				// Custom alias (excluding blank imports)
				prompt.WriteString(fmt.Sprintf("- %s \"%s\"\n", imp.Alias, imp.Path))
			} else {
				// Package with different identifier
				prompt.WriteString(fmt.Sprintf("- %s \"%s\"\n", identifier, imp.Path))
			}
		}
		prompt.WriteString("\n")
	}

	// 関数シグネチャに関連する型情報を優先的に表示
	if len(ctx.Types) > 0 {
		prompt.WriteString("Available types:\n")
		for typeName, typeDef := range ctx.Types {
			prompt.WriteString(fmt.Sprintf("```go\n%s\n```\n", typeDef))

			// Include methods for this type if available
			if methods, exists := ctx.Methods[typeName]; exists && len(methods) > 0 {
				prompt.WriteString("\nMethods:\n")
				for _, method := range methods {
					prompt.WriteString(fmt.Sprintf("- %s\n", method.Signature))
				}
			}
			prompt.WriteString("\n")
		}
	}

	prompt.WriteString("</context>\n\n")

	prompt.WriteString("<target>\n")
	prompt.WriteString(fmt.Sprintf("```go\n%s {\n    <IMPLEMENT_HERE>\n}\n```\n", target.GetFunctionSignature()))
	prompt.WriteString("</target>\n\n")

	prompt.WriteString("<instruction>\n")
	prompt.WriteString(fmt.Sprintf("%s\n", target.Instruction))
	prompt.WriteString("</instruction>\n")

	// Add additional context if provided
	if b.additionalContext != "" {
		prompt.WriteString("\n<additional_context>\n")
		prompt.WriteString(b.additionalContext)
		prompt.WriteString("\n</additional_context>\n")
	}

	fullPrompt := prompt.String()

	return fullPrompt
}
