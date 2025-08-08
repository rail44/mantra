package phase

import (
	"sync"

	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/impl"
	"github.com/rail44/mantra/internal/tools/schemas"
)

// ImplementationPhase represents the phase where AI generates the actual code
type ImplementationPhase struct {
	temperature float32
	tools       []tools.Tool
	projectRoot string
	logger      log.Logger
	result      interface{}
	completed   bool
	mu          sync.Mutex
}

// NewImplementationPhase creates a new implementation phase
func NewImplementationPhase(temperature float32, projectRoot string, logger log.Logger) *ImplementationPhase {
	if logger == nil {
		logger = log.Default()
	}

	phase := &ImplementationPhase{
		temperature: temperature,
		projectRoot: projectRoot,
		logger:      logger,
	}

	// Initialize tools for implementation/validation
	tools := []tools.Tool{
		impl.NewCheckCodeTool(projectRoot),
		impl.NewResultTool(
			"implementation",
			&schemas.ImplementationResultSchema{},
			phase.storeResult,
		),
	}

	phase.tools = tools
	return phase
}

// storeResult stores the result from the result tool
func (p *ImplementationPhase) storeResult(result interface{}) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.result = result
	p.completed = true
	return nil
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
After validation, call the result() tool with JSON containing:

### For successful implementation:
{
  "success": true,
  "code": "// Your implementation code here\n// Multiple lines of Go code\n// That goes INSIDE the function body"
}

Important for the code field:
- Pass ONLY the implementation code that goes INSIDE the function body
- Do NOT include the function signature (func name(...) ...)
- Do NOT include type definitions or constants
- Do NOT include the opening and closing braces of the function
- Do NOT wrap in markdown code blocks
- Just the pure Go statements that replace <IMPLEMENT_HERE>

### For failures:
{
  "success": false,
  "error": {
    "message": "Brief description of what prevented implementation",
    "details": "Specific missing items, what was found instead, what's needed to proceed"
  }
}

## Example Error Cases
- Missing method: "Method 'GetUserByID' not found on 'UserService'"
  Details: "Found 'GetUser' and 'GetUserByEmail' instead. Need the exact method signature."
- Missing type: "Return type 'ValidationResult' not defined"
  Details: "Need import path or type definition for ValidationResult"
- Unclear requirements: "Instruction requires 'cache TTL' but not specified"
  Details: "No duration specified in instruction and no default found in codebase"

## Important
- ALWAYS call the result() tool to complete the phase
- Use success: false when you cannot generate valid implementation
- Provide specific, actionable error messages`
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

// GetResult returns the phase result and whether it's complete
func (p *ImplementationPhase) GetResult() (interface{}, bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.result, p.completed
}

// Reset clears the phase state for reuse
func (p *ImplementationPhase) Reset() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.result = nil
	p.completed = false
}
