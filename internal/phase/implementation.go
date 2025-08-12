package phase

import (
	"encoding/json"
	"fmt"
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
	schema      schemas.ResultSchema
	setStep     StepCallback
}

// NewImplementationPhase creates a new implementation phase
func NewImplementationPhase(temperature float32, projectRoot string, logger log.Logger, setStep StepCallback) *ImplementationPhase {
	if logger == nil {
		logger = log.Default()
	}

	phase := &ImplementationPhase{
		temperature: temperature,
		projectRoot: projectRoot,
		logger:      logger,
		schema:      &implementationResultSchema{},
		setStep:     setStep,
	}

	// Initialize tools for implementation/validation
	tools := []tools.Tool{
		impl.NewCheckCodeTool(projectRoot),
		impl.NewResultTool(
			"implementation",
			phase.schema,
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

// Name returns the name of this phase
func (p *ImplementationPhase) Name() string {
	return "Implementation"
}

// Temperature returns the temperature for implementation (lower for accuracy)
func (p *ImplementationPhase) Temperature() float32 {
	return p.temperature
}

// Tools returns the implementation/validation tools
func (p *ImplementationPhase) Tools() []tools.Tool {
	return p.tools
}

// SystemPrompt returns the system prompt for implementation
func (p *ImplementationPhase) SystemPrompt() string {
	return `You are an expert Go developer. Your task: generate ONLY the code that replaces <IMPLEMENT_HERE>.

## Input Structure
- <target>: The function signature to implement
- <context>: Initial context from function signature
	- Receiver and parameter type definitions
	- Implemented methods for each type (excluding the method being implemented)
  - Already imported packages
- <instruction>: Natural language description of what the function should do
- <instruction>: Natural language description of what the function should do
- <additional_context>: Additional context from previous exploration phase, if available

## Available Tool

- check_code(): Validate your code syntax and structure
- result(): Submit the final result and complete this phase

## Process

1. Review all information in <context> and <additional_context>
2. Implement according to <instruction> using available types and functions
3. Validate your implementation with check_code tool
4. Fix any issues found by the analysis
5. After finalize, call the result() tool

## Result Tool Usage

Call result() with JSON containing:

### For successful gathering:

{
  "success": true,
  "code": "..."  // Your generated function body
}

### For failures:
{
  "success": false,
  "error": {
    "message": "Brief description of what prevented implementation",
    "details": "Specific missing items, what was found instead, what's needed to proceed"
  }
}

## Important

- ALWAYS call the result() tool to complete the phase
- Use success: false when you cannot gather enough context
- Provide clear error messages to help diagnose issues`
}

// PromptBuilder returns a prompt builder configured for implementation
func (p *ImplementationPhase) PromptBuilder() *prompt.Builder {
	builder := prompt.NewBuilder(p.logger)
	builder.SetUseTools(true) // Still uses tools (check_syntax)
	return builder
}

// PromptBuilderWithContext returns a prompt builder with additional context from previous phase
func (p *ImplementationPhase) PromptBuilderWithContext(contextResult string) *prompt.Builder {
	builder := prompt.NewBuilder(p.logger)
	builder.SetUseTools(true)

	// Format the context result appropriately
	formattedContext := "## Additional Context from Exploration:\n" + contextResult
	return builder.WithAdditionalContext(formattedContext)
}

// Result returns the phase result and whether it's complete
func (p *ImplementationPhase) Result() (interface{}, bool) {
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

// ResultSchema returns the schema for this phase's result tool
func (p *ImplementationPhase) ResultSchema() schemas.ResultSchema {
	return p.schema
}

// implementationResultSchema defines the schema for implementation phase results
type implementationResultSchema struct{}

// Schema returns the JSON schema for implementation results
func (s *implementationResultSchema) Schema() json.RawMessage {
	return json.RawMessage(`{
		"type": "object",
		"properties": {
			"success": {
				"type": "boolean",
				"description": "Whether the implementation generation was successful"
			},
			"code": {
				"type": "string",
				"description": "The generated Go code implementation"
			},
			"error": {
				"type": "object",
				"properties": {
					"message": {
						"type": "string",
						"description": "Error message explaining what went wrong"
					},
					"details": {
						"type": "string",
						"description": "Additional details about what was missing or failed"
					}
				},
				"required": ["message"],
				"additionalProperties": false
			}
		},
		"required": ["success"],
		"additionalProperties": false
	}`)
}

// Validate checks if the data conforms to the implementation schema
func (s *implementationResultSchema) Validate(data interface{}) error {
	// Basic type check
	dataMap, ok := data.(map[string]interface{})
	if !ok {
		return fmt.Errorf("expected object, got %T", data)
	}

	// Check for required "success" field
	success, ok := dataMap["success"]
	if !ok {
		return fmt.Errorf("missing required field: success")
	}

	successBool, ok := success.(bool)
	if !ok {
		return fmt.Errorf("success must be a boolean, got %T", success)
	}

	// If failed, check for error field
	if !successBool {
		errorField, ok := dataMap["error"]
		if !ok {
			return fmt.Errorf("error field is required when success is false")
		}

		errorMap, ok := errorField.(map[string]interface{})
		if !ok {
			return fmt.Errorf("error must be an object, got %T", errorField)
		}

		if _, ok := errorMap["message"].(string); !ok {
			return fmt.Errorf("error.message must be a string")
		}

		return nil // Valid error response
	}

	// For successful results, check for required "code" field
	code, ok := dataMap["code"]
	if !ok {
		return fmt.Errorf("missing required field: code when success is true")
	}

	// Validate that code is a string
	if _, ok := code.(string); !ok {
		return fmt.Errorf("code must be a string, got %T", code)
	}

	// Check that code is not empty
	if codeStr := code.(string); codeStr == "" {
		return fmt.Errorf("code cannot be empty")
	}

	return nil
}

// Transform converts the raw data into ImplementationResult
func (s *implementationResultSchema) Transform(data interface{}) (interface{}, error) {
	dataMap := data.(map[string]interface{})

	// Return the entire map to preserve success/error information
	// The cmd/generate.go will handle the structure appropriately
	return dataMap, nil
}
