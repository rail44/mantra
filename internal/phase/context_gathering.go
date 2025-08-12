package phase

import (
	"encoding/json"
	"fmt"
	"log/slog"
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
	logger      *slog.Logger
	result      interface{}
	completed   bool
	mu          sync.Mutex
	schema      schemas.ResultSchema
	setStep     StepCallback
}

// NewContextGatheringPhase creates a new context gathering phase
func NewContextGatheringPhase(temperature float32, packagePath string, logger *slog.Logger, setStep StepCallback) *ContextGatheringPhase {
	if logger == nil {
		logger = log.Default()
	}

	phase := &ContextGatheringPhase{
		temperature: temperature,
		logger:      logger,
		schema:      &contextGatheringResultSchema{},
		setStep:     setStep,
	}

	// Initialize tools for context gathering (limited to current package)
	tools := []tools.Tool{
		impl.NewInspectTool(packagePath), // Use go/packages for accurate type info including implementations
		impl.NewResultTool(
			"context gathering",
			phase.schema,
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

// Name returns the name of this phase
func (p *ContextGatheringPhase) Name() string {
	return "Context Gathering"
}

// Temperature returns the temperature for context gathering (higher for exploration)
func (p *ContextGatheringPhase) Temperature() float32 {
	return p.temperature
}

// Tools returns the context gathering tools
func (p *ContextGatheringPhase) Tools() []tools.Tool {
	return p.tools
}

// SystemPrompt returns the system prompt for context gathering
func (p *ContextGatheringPhase) SystemPrompt() string {
	return `You are a Go code analyzer gathering code context to implement a function.

## Input Structure

- <target>: The function signature to implement
- <context>: Initial context from function signature
	- Receiver and parameter type definitions
	- Implemented methods for each type (excluding the method being implemented)
  - Already imported packages
- <instruction>: Natural language description of what the function should do

## Available Tools

- inspect(): Get detail of identifier
	- types, package, function and variable from current scope
- result(): Submit the final result and complete this phase

## Process
1. Gather additional context using the tools
	- Use inspect() to get details of unclear identifier
	- Prevent to use inspect() on standard library unless necessary
2. When you have enough context or cannot proceed, call the result() tool

## Result Tool Usage

Call result() with JSON containing:

### For successful gathering:

All fields should be include only new context gathered

{
  "success": true,
  "types": [...],      // Array of type definitions found
  "functions": [...],  // Array of function signatures/implementations found
  "constants": [...]   // Array of constant/variable definitions found
}

### For failures:

{
  "success": false,
  "error": {
    "message": "Brief description of what went wrong",
    "details": "What you were looking for, what you found instead, what's needed to proceed"
  }
}

## Important

- ALWAYS call the result() tool to complete the phase
- Use success: false when you cannot gather enough context
- Provide clear error messages to help diagnose issues`
}

// PromptBuilder returns a prompt builder configured for context gathering
func (p *ContextGatheringPhase) PromptBuilder() *prompt.Builder {
	builder := prompt.NewBuilder(p.logger)
	builder.SetUseTools(true)
	return builder
}

// Result returns the phase result and whether it's complete
func (p *ContextGatheringPhase) Result() (interface{}, bool) {
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

// ResultSchema returns the schema for this phase's result tool
func (p *ContextGatheringPhase) ResultSchema() schemas.ResultSchema {
	return p.schema
}

// contextGatheringResultSchema defines the schema for context gathering phase results
type contextGatheringResultSchema struct{}

// Schema returns the JSON schema for context gathering results
func (s *contextGatheringResultSchema) Schema() json.RawMessage {
	return json.RawMessage(`{
		"type": "object",
		"properties": {
			"success": {
				"type": "boolean",
				"description": "Whether the context gathering was successful"
			},
			"types": {
				"type": "array",
				"items": {
					"type": "object",
					"properties": {
						"name": {"type": "string"},
						"definition": {"type": "string"},
						"methods": {
							"type": "array",
							"items": {"type": "string"}
						}
					},
					"required": ["name", "definition"],
					"additionalProperties": false
				}
			},
			"functions": {
				"type": "array",
				"items": {
					"type": "object",
					"properties": {
						"name": {"type": "string"},
						"signature": {"type": "string"},
						"implementation": {"type": "string"}
					},
					"required": ["name", "signature"],
					"additionalProperties": false
				}
			},
			"constants": {
				"type": "array",
				"items": {
					"type": "object",
					"properties": {
						"name": {"type": "string"},
						"type": {"type": "string"},
						"value": {"type": "string"}
					},
					"required": ["name"],
					"additionalProperties": false
				}
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
						"description": "Additional details about the error"
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

// Validate checks if the data conforms to the context gathering schema
func (s *contextGatheringResultSchema) Validate(data interface{}) error {
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

	// For successful results, check for at least one of types, functions, or constants
	hasTypes := dataMap["types"] != nil
	hasFunctions := dataMap["functions"] != nil
	hasConstants := dataMap["constants"] != nil

	if !hasTypes && !hasFunctions && !hasConstants {
		return fmt.Errorf("successful result must contain at least one of: types, functions, or constants")
	}

	// Validate types array if present
	if types, ok := dataMap["types"].([]interface{}); ok {
		for i, t := range types {
			typeMap, ok := t.(map[string]interface{})
			if !ok {
				return fmt.Errorf("types[%d] must be an object", i)
			}
			if _, ok := typeMap["name"].(string); !ok {
				return fmt.Errorf("types[%d].name must be a string", i)
			}
			if _, ok := typeMap["definition"].(string); !ok {
				return fmt.Errorf("types[%d].definition must be a string", i)
			}
		}
	}

	// Validate functions array if present
	if functions, ok := dataMap["functions"].([]interface{}); ok {
		for i, f := range functions {
			funcMap, ok := f.(map[string]interface{})
			if !ok {
				return fmt.Errorf("functions[%d] must be an object", i)
			}
			if _, ok := funcMap["name"].(string); !ok {
				return fmt.Errorf("functions[%d].name must be a string", i)
			}
			if _, ok := funcMap["signature"].(string); !ok {
				return fmt.Errorf("functions[%d].signature must be a string", i)
			}
		}
	}

	// Validate constants array if present
	if constants, ok := dataMap["constants"].([]interface{}); ok {
		for i, c := range constants {
			constMap, ok := c.(map[string]interface{})
			if !ok {
				return fmt.Errorf("constants[%d] must be an object", i)
			}
			if _, ok := constMap["name"].(string); !ok {
				return fmt.Errorf("constants[%d].name must be a string", i)
			}
		}
	}

	return nil
}

// Transform converts the raw data into ContextGatheringResult
func (s *contextGatheringResultSchema) Transform(data interface{}) (interface{}, error) {
	// Return the entire map to preserve success/error information
	// The cmd/generate.go will handle the structure appropriately
	return data, nil
}
