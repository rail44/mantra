package phase

import (
	"sync"

	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/impl"
	"github.com/rail44/mantra/internal/tools/schemas"
)

// ContextGatheringPhase represents the phase where AI explores the codebase
type ContextGatheringPhase struct {
	temperature float32
	tools       []tools.Tool
	logger      log.Logger
	result      interface{}
	completed   bool
	mu          sync.Mutex
}

// NewContextGatheringPhase creates a new context gathering phase
func NewContextGatheringPhase(temperature float32, packagePath string, logger log.Logger) *ContextGatheringPhase {
	if logger == nil {
		logger = log.Default()
	}

	phase := &ContextGatheringPhase{
		temperature: temperature,
		logger:      logger,
	}

	// Initialize tools for context gathering (limited to current package)
	tools := []tools.Tool{
		impl.NewInspectTool(packagePath), // Use go/packages for accurate type info including implementations
		impl.NewResultTool(
			"context gathering",
			&schemas.ContextGatheringResultSchema{},
			phase.storeResult,
		),
	}

	phase.tools = tools
	return phase
}

// storeResult stores the result from the result tool
func (p *ContextGatheringPhase) storeResult(result interface{}) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.result = result
	p.completed = true
	return nil
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
1. Gather additional context using the tools
	- Use inspect() to get details of unclear identifier
	- Prevent to use inspect() on standard library unless necessary
2. When you have enough context to implement the function, call the result() tool
3. The result() tool should be called with structured data containing:
	- types: Array of type definitions found
	- functions: Array of function signatures/implementations found
	- constants: Array of constant/variable definitions found


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

// GetResult returns the phase result and whether it's complete
func (p *ContextGatheringPhase) GetResult() (interface{}, bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.result, p.completed
}

// Reset clears the phase state for reuse
func (p *ContextGatheringPhase) Reset() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.result = nil
	p.completed = false
}
