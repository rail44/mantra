package phase

import (
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/impl"
)

// ImplementationPhase represents the phase where AI generates the actual code
type ImplementationPhase struct {
	temperature float32
	tools       []tools.Tool
}

// NewImplementationPhase creates a new implementation phase
func NewImplementationPhase(temperature float32) *ImplementationPhase {
	// Initialize tools for implementation/validation
	tools := []tools.Tool{
		impl.NewCheckSyntaxTool(),
	}

	return &ImplementationPhase{
		temperature: temperature,
		tools:       tools,
	}
}

// GetTemperature returns the temperature for implementation (lower for accuracy)
func (p *ImplementationPhase) GetTemperature() float32 {
	return p.temperature
}

// GetTools returns the implementation/validation tools
func (p *ImplementationPhase) GetTools() []tools.Tool {
	return p.tools
}

// GetSystemPrompt returns the system prompt for implementation
func (p *ImplementationPhase) GetSystemPrompt() string {
	return `You are an expert Go developer. Your task: generate ONLY the code that replaces <IMPLEMENT_HERE>.

## Input Structure
- <context>: Complete context including all types, functions, and imports discovered in the previous phase
- <target>: The function signature with <IMPLEMENT_HERE> placeholder
- <instruction>: Natural language description of what the function should do

## Available Tool
- **check_syntax**: Validate the syntax of your generated code (parameter: code)

## Process
1. Review all information in <context>
2. Implement according to <instruction> using available types and functions
3. Validate your implementation with check_syntax tool
4. Only proceed if you receive {"valid": true}

## Output Format
After successful validation, return ONLY the implementation code.
No explanations, no markdown code blocks, no comments - just pure Go code that directly replaces <IMPLEMENT_HERE>. `
}

// GetPromptBuilder returns a prompt builder configured for implementation
func (p *ImplementationPhase) GetPromptBuilder() *prompt.Builder {
	builder := prompt.NewBuilder()
	builder.SetUseTools(true) // Still uses tools (check_syntax)
	return builder
}
