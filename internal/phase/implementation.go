package phase

import (
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/impl"
)

// ImplementationPhase represents the phase where AI generates the actual code
type ImplementationPhase struct {
	temperature float32
	tools       []tools.Tool
	projectRoot string
	logger      log.Logger
}

// NewImplementationPhase creates a new implementation phase
func NewImplementationPhase(temperature float32, projectRoot string, logger log.Logger) *ImplementationPhase {
	if logger == nil {
		logger = log.Default()
	}

	// Initialize tools for implementation/validation
	tools := []tools.Tool{
		impl.NewCheckCodeTool(projectRoot),
	}

	return &ImplementationPhase{
		temperature: temperature,
		tools:       tools,
		projectRoot: projectRoot,
		logger:      logger,
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
- <context>: Complete context including:
  - Available imports: Packages already imported and in use
  - Additional packages (blank imports): Available for use IF the instructions explicitly mention them
  - Types: All relevant type definitions
- <target>: The function signature with <IMPLEMENT_HERE> placeholder
- <instruction>: Natural language description of what the function should do

## Available Tool
- **check_code**: Comprehensive validation including syntax and static analysis (parameter: code)

## Process
1. Review all information in <context>
2. Implement according to <instruction> using available types and functions
   - For methods: directly access struct fields (e.g., c.items[key], not c.Get(key))
   - Use standard library functions directly (e.g., time.Now())
   - The method being implemented is NOT available to call (would cause recursion)
3. Validate your implementation with check_code tool
4. Fix any issues found by the analysis
5. Only return code that passes validation

## Output Format
After successful validation, return ONLY the implementation code that goes INSIDE the function body.
- Do NOT include the function signature (func name(...) ...)
- Do NOT include type definitions or constants
- Do NOT include the opening and closing braces of the function
- Do NOT wrap in markdown code blocks
- Just the pure Go statements that replace <IMPLEMENT_HERE>

## Error Handling
If generation cannot proceed, respond with: GENERATION_FAILED: [reason]

Include in the reason:
- What you were looking for
- Where you searched
- What you found instead (if relevant)
- What information is needed to proceed

Examples:
  - GENERATION_FAILED: Method 'GetUserByID' not found on 'UserService' - found 'GetUser' and 'GetUserByEmail' instead
  - GENERATION_FAILED: Return type 'ValidationResult' not defined - need import path or type definition
  - GENERATION_FAILED: Instruction requires 'cache TTL' but no duration specified and no default found in codebase`
}

// GetPromptBuilder returns a prompt builder configured for implementation
func (p *ImplementationPhase) GetPromptBuilder() *prompt.Builder {
	builder := prompt.NewBuilder(p.logger)
	builder.SetUseTools(true) // Still uses tools (check_syntax)
	return builder
}

// GetPromptBuilderWithContext returns a prompt builder with additional context from previous phase
func (p *ImplementationPhase) GetPromptBuilderWithContext(contextResult string) *prompt.Builder {
	builder := prompt.NewBuilder(p.logger)
	builder.SetUseTools(true)

	// Format the context result appropriately
	formattedContext := "## Additional Context from Exploration:\n" + contextResult
	return builder.WithAdditionalContext(formattedContext)
}
