package phase

import (
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/impl"
)

// ContextGatheringPhase represents the phase where AI explores the codebase
type ContextGatheringPhase struct {
	temperature float32
	tools       []tools.Tool
	logger      log.Logger
}

// NewContextGatheringPhase creates a new context gathering phase
func NewContextGatheringPhase(temperature float32, projectRoot string, logger log.Logger) *ContextGatheringPhase {
	if logger == nil {
		logger = log.Default()
	}
	
	// Initialize tools for context gathering
	tools := []tools.Tool{
		impl.NewInspectTool(),
		impl.NewSearchTool(projectRoot),
		impl.NewReadFuncTool(projectRoot),
	}

	return &ContextGatheringPhase{
		temperature: temperature,
		tools:       tools,
		logger:      logger,
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
- search: Find code patterns in current package (parameter: pattern)
- read_func: Read function implementations (parameter: name)

## Process
1. Use search() tool to find type definitions if needed
2. Use inspect() tool to get unclear types, variables, and constants
3. Use read_func() tool to see details of existing functions
4. Continue above steps until you have enough context
5. Return your findings in the format below
6. Do not return any other text or explanations

## Output Format

` + "```" + `markdown
### Types Found:
<found types>

### Functions Found:
<found functions>

### Constants/Variables Found:
<found constants/variables>

### Additional Imports Required:
<found additional imports>
` + "```" + `

Each section should be formatted as Go code blocks.`
}

// GetPromptBuilder returns a prompt builder configured for context gathering
func (p *ContextGatheringPhase) GetPromptBuilder() *prompt.Builder {
	builder := prompt.NewBuilder(p.logger)
	builder.SetUseTools(true)
	return builder
}
