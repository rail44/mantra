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
	useTools bool
}

// NewBuilder creates a new prompt builder
func NewBuilder() *Builder {
	return &Builder{}
}

// SetUseTools enables or disables tool usage instructions in prompts
func (b *Builder) SetUseTools(useTools bool) {
	b.useTools = useTools
}

// BuildForTarget creates a prompt for a specific generation target
func (b *Builder) BuildForTarget(target *parser.Target, fileContent string) (string, error) {
	// Use project-wide context extraction by default
	projectCtx, err := context.ExtractProjectContext(target.FilePath, target)
	if err != nil {
		log.Error("package has compilation errors", slog.String("error", err.Error()))
		log.Error("fix compilation errors before running generation")
		return "", fmt.Errorf("package compilation failed: %w", err)
	}

	return b.buildPromptWithContext(&projectCtx.RelevantContext, target), nil
}

// buildPromptWithContext builds a prompt using the extracted context
func (b *Builder) buildPromptWithContext(ctx *context.RelevantContext, target *parser.Target) string {
	var prompt strings.Builder

	// Show the function with placeholder
	prompt.WriteString("Implement the following function:\n\n")
	prompt.WriteString(fmt.Sprintf("%s {\n    <IMPLEMENT_HERE>\n}\n\n", target.GetFunctionSignature()))

	// Add the mantra instruction
	prompt.WriteString(fmt.Sprintf("Instruction: %s\n", target.Instruction))

	// Only add minimal context when needed
	if len(ctx.Types) > 0 {
		prompt.WriteString("\nContext:\n")
		for _, typeDef := range ctx.Types {
			prompt.WriteString(fmt.Sprintf("- %s\n", typeDef))
		}
	}

	fullPrompt := prompt.String()

	// Log the generated prompt at trace level for debugging
	log.Trace("generated prompt",
		slog.String("function", target.Name),
		slog.Int("length", len(fullPrompt)))

	return fullPrompt
}
