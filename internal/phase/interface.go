package phase

import (
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
)

// Phase represents a generation phase with its own configuration
type Phase interface {
	// GetTemperature returns the temperature for this phase
	GetTemperature() float32

	// GetTools returns the tools available in this phase
	GetTools() []tools.Tool

	// GetSystemPrompt returns the system prompt for this phase
	GetSystemPrompt() string

	// GetPromptBuilder returns a configured prompt builder for this phase
	GetPromptBuilder() *prompt.Builder
}
