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
func NewContextGatheringPhase(temperature float32, packagePath string, logger log.Logger) *ContextGatheringPhase {
	if logger == nil {
		logger = log.Default()
	}

	// Initialize tools for context gathering (limited to current package)
	tools := []tools.Tool{
		impl.NewInspectTool(packagePath), // Use go/packages for accurate type info including implementations
		impl.NewSearchTool(packagePath),  // Search only in current package
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
	return `You are a Go code analyzer gathering code context to implement a function.

## Input Structure
- <target>: The function signature to implement
- <context>: Initial context from function signature
	- receiver and parameter type definitions
	- implemented methods for each type (excluding the method being implemented)
- <instruction>: Natural language description of what the function should do

## Available Tools
- inspect(name): Get details of types and functions from the current package
- search(pattern): Search for declarations by pattern from the current package

## Important Guidelines
1. DO NOT search for standard library functions (e.g., time.Now, fmt.Sprintf)
2. DO NOT search for functions from imported packages - they are already available
3. ONLY search for types, functions, and constants defined in the CURRENT package
4. Focus on understanding the data structures and methods you need to use

## Process
1. Gather additional context using the tools, until you have enough information to implement the function.
	- Use inspect() to get details of types, functions, constants, and variables from the current package
	- Use search() to find declarations from the current package when you don't know the exact name
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
- Where you searched
- What you found instead (if relevant)
- What information is needed to proceed`
}

// GetPromptBuilder returns a prompt builder configured for context gathering
func (p *ContextGatheringPhase) GetPromptBuilder() *prompt.Builder {
	builder := prompt.NewBuilder(p.logger)
	builder.SetUseTools(true)
	return builder
}
