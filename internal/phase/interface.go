package phase

import (
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/schemas"
)

// Phase represents a generation phase with its own configuration
type Phase interface {
	// Name returns the name of this phase
	Name() string

	// Temperature returns the temperature for this phase
	Temperature() float32

	// Tools returns the tools available in this phase
	Tools() []tools.Tool

	// SystemPrompt returns the system prompt for this phase
	SystemPrompt() string

	// PromptBuilder returns a configured prompt builder for this phase
	PromptBuilder() *prompt.Builder

	// Result returns the phase result and whether it's complete
	Result() (any, bool)

	// Reset clears the phase state for reuse
	Reset()

	// ResultSchema returns the schema for this phase's result tool
	ResultSchema() schemas.ResultSchema
}
