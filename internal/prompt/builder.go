package prompt

import (
	"fmt"
	"log/slog"
	"strings"

	"github.com/rail44/mantra/internal/context"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/parser"
)

// Builder creates prompts for code generation
type Builder struct {
	useTools          bool
	additionalContext string
	logger            log.Logger
}

// NewBuilder creates a new prompt builder
func NewBuilder(logger log.Logger) *Builder {
	if logger == nil {
		logger = log.Default()
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

	// Import情報を整理して表示
	var regularImports []*context.ImportInfo
	var blankImports []*context.ImportInfo

	for _, imp := range ctx.Imports {
		if imp.IsBlank {
			blankImports = append(blankImports, imp)
		} else {
			regularImports = append(regularImports, imp)
		}
	}

	// Regular imports
	if len(regularImports) > 0 {
		prompt.WriteString("Available imports (already in use):\n```go\n")
		for _, imp := range regularImports {
			if imp.Alias != "" && imp.Alias != imp.GetIdentifier() {
				prompt.WriteString(fmt.Sprintf("import %s \"%s\"\n", imp.Alias, imp.Path))
			} else {
				prompt.WriteString(fmt.Sprintf("import \"%s\"\n", imp.Path))
			}
		}
		prompt.WriteString("```\n")
		prompt.WriteString("Usage in code:\n")
		for _, imp := range regularImports {
			prompt.WriteString(fmt.Sprintf("- %s (from %s)\n", imp.GetIdentifier(), imp.Path))
		}
		prompt.WriteString("\n")
	}

	// Blank imports indicate packages available for generated code
	if len(blankImports) > 0 {
		prompt.WriteString("Additional packages available for generated code (marked with _ import):\n")
		for _, imp := range blankImports {
			prompt.WriteString(fmt.Sprintf("- %s (use as: %s)\n", imp.Path, imp.GetIdentifier()))
		}
		prompt.WriteString("Use these packages IF needed based on the specific instructions for this function.\n\n")
	}

	// 関数シグネチャに関連する型情報を優先的に表示
	if len(ctx.Types) > 0 {
		prompt.WriteString("Available types:\n")
		for _, typeDef := range ctx.Types {
			prompt.WriteString(fmt.Sprintf("```go\n%s\n```\n\n", typeDef))
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

	// Log the generated prompt at trace level for debugging
	b.logger.Trace(fmt.Sprintf("[PROMPT] %s: %d chars, %d types, %d imports",
		target.Name, len(fullPrompt), len(ctx.Types), len(ctx.Imports)))

	// Log imports separately for debugging
	if len(ctx.Imports) > 0 {
		var importPaths []string
		for _, imp := range ctx.Imports {
			if imp.IsBlank {
				importPaths = append(importPaths, "_ "+imp.Path)
			} else if imp.Alias != "" {
				importPaths = append(importPaths, imp.Alias+" "+imp.Path)
			} else {
				importPaths = append(importPaths, imp.Path)
			}
		}
		b.logger.Trace(fmt.Sprintf("         imports: %v", importPaths))
	}

	return fullPrompt
}
