package phase

import (
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/impl"
)

// ContextGatheringPhase represents the phase where AI explores the codebase
type ContextGatheringPhase struct {
	temperature float32
	tools       []tools.Tool
}

// NewContextGatheringPhase creates a new context gathering phase
func NewContextGatheringPhase(temperature float32, projectRoot string) *ContextGatheringPhase {
	// Initialize tools for context gathering
	tools := []tools.Tool{
		impl.NewInspectTool(),
		impl.NewSearchTool(projectRoot),
		impl.NewReadFuncTool(projectRoot),
	}
	
	return &ContextGatheringPhase{
		temperature: temperature,
		tools:       tools,
	}
}

// GetTemperature returns the temperature for context gathering (higher for exploration)
func (p *ContextGatheringPhase) GetTemperature() float32 {
	return p.temperature
}

// GetTools returns the context gathering tools
func (p *ContextGatheringPhase) GetTools() []tools.Tool {
	return p.tools
}

// GetSystemPrompt returns the system prompt for context gathering
func (p *ContextGatheringPhase) GetSystemPrompt() string {
	return `You are a Go code analyzer gathering code context for implementation.

## Input Structure
- <context>: Initial context with available types and imports
- <target>: The function signature to implement
- <instruction>: Natural language description of what the function should do

## Available Tools
- inspect: Get struct/interface definitions (parameter: name)
- search: Find code patterns (parameter: pattern)
- read_func: Read function implementations (parameter: name)

## Process
1. Look at the target function and identify all types mentioned
2. For each type, use inspect to get its definition
3. Use search to find methods and related code
4. Use read_func for similar function implementations

## Output Format
Output the actual Go code found, organized in sections:

### Types Found:
(paste type definitions here)

### Functions Found:
(paste function implementations here)

### Constants/Variables Found:
(paste constants and variables here)

### Additional Imports Required:
(paste import statements here)

Start your response directly with "### Types Found:"`
}

// GetPromptBuilder returns a prompt builder configured for context gathering
func (p *ContextGatheringPhase) GetPromptBuilder() *prompt.Builder {
	builder := prompt.NewBuilder()
	builder.SetUseTools(true)
	return builder
}