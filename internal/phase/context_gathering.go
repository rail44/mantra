package phase

import (
	"github.com/rail44/mantra/internal/ai"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/impl"
)

// ContextGatheringPhase represents the phase where AI explores the codebase
type ContextGatheringPhase struct {
	temperature      float32
	tools            []tools.Tool
	logger           log.Logger
	structuredOutput bool
}

// NewContextGatheringPhase creates a new context gathering phase
func NewContextGatheringPhase(temperature float32, packagePath string, logger log.Logger, structuredOutput bool) *ContextGatheringPhase {
	if logger == nil {
		logger = log.Default()
	}

	// Initialize tools for context gathering (limited to current package)
	tools := []tools.Tool{
		impl.NewInspectTool(packagePath), // Use go/packages for accurate type info including implementations
	}

	return &ContextGatheringPhase{
		temperature:      temperature,
		tools:            tools,
		logger:           logger,
		structuredOutput: structuredOutput,
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
	return `You are a Go code analyzer gathering code context to implement a function.

## Input Structure
- <target>: The function signature to implement
- <context>: Initial context from function signature
	- receiver and parameter type definitions
	- implemented methods for each type (excluding the method being implemented)
- <instruction>: Natural language description of what the function should do

## Available Tools
- inspect(name): Get details of types, package, function and variable from current scope

## Process
1. Gather additional context using the tools, until you have enough information to implement the function.
	- Use inspect() to get details of unclear identifier
2. Return your findings as additional context in the format below
3. Do not return any other text or explanations

## Output Format

` + "```" + `markdown
### Types

#### <found additional type name>

<definition of the type>

<methods of the type>

### Functions

#### <found additional function name>

<definition of the function>

<implementation of the function if you found it>

### Constants/Variables

#### <found additional constant/variable name>

<definition of the constant/variable>
` + "```" + `

Each section should be formatted as Go code blocks.

## Error Handling
If generation cannot proceed, respond with: GENERATION_FAILED: [reason]

Include in the reason:
- What you were looking for
- What you found instead (if relevant)
- What information is needed to proceed`
}

// GetPromptBuilder returns a prompt builder configured for context gathering
func (p *ContextGatheringPhase) GetPromptBuilder() *prompt.Builder {
	builder := prompt.NewBuilder(p.logger)
	builder.SetUseTools(true)
	return builder
}

// GetResponseFormat returns the structured output format for context gathering
func (p *ContextGatheringPhase) GetResponseFormat() *ai.ResponseFormat {
	if !p.structuredOutput {
		return nil
	}
	return &ai.ResponseFormat{
		Type:       "json_schema",
		JSONSchema: ai.ContextGatheringSchema,
	}
}
