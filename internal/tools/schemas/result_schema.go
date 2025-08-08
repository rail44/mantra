package schemas

import (
	"encoding/json"
	"fmt"
)

// ResultSchema defines the interface for phase result schemas
type ResultSchema interface {
	// GetSchema returns the JSON schema for validation
	GetSchema() json.RawMessage

	// Validate checks if the data conforms to the schema
	Validate(data interface{}) error

	// Transform converts the raw data into the appropriate structure
	Transform(data interface{}) (interface{}, error)
}

// ContextGatheringResultSchema defines the schema for context gathering phase results
type ContextGatheringResultSchema struct{}

// GetSchema returns the JSON schema for context gathering results
func (s *ContextGatheringResultSchema) GetSchema() json.RawMessage {
	return json.RawMessage(`{
		"type": "object",
		"properties": {
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
			}
		},
		"additionalProperties": false
	}`)
}

// Validate checks if the data conforms to the context gathering schema
func (s *ContextGatheringResultSchema) Validate(data interface{}) error {
	// Basic type check
	dataMap, ok := data.(map[string]interface{})
	if !ok {
		return fmt.Errorf("expected object, got %T", data)
	}

	// Check for at least one of types, functions, or constants
	hasTypes := dataMap["types"] != nil
	hasFunctions := dataMap["functions"] != nil
	hasConstants := dataMap["constants"] != nil

	if !hasTypes && !hasFunctions && !hasConstants {
		return fmt.Errorf("result must contain at least one of: types, functions, or constants")
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
func (s *ContextGatheringResultSchema) Transform(data interface{}) (interface{}, error) {
	// For now, just return the validated data as-is
	// In the future, we could transform this into a strongly-typed struct
	return data, nil
}

// ImplementationResultSchema defines the schema for implementation phase results
type ImplementationResultSchema struct{}

// GetSchema returns the JSON schema for implementation results
func (s *ImplementationResultSchema) GetSchema() json.RawMessage {
	return json.RawMessage(`{
		"type": "object",
		"properties": {
			"code": {
				"type": "string",
				"description": "The generated Go code implementation"
			}
		},
		"required": ["code"],
		"additionalProperties": false
	}`)
}

// Validate checks if the data conforms to the implementation schema
func (s *ImplementationResultSchema) Validate(data interface{}) error {
	// Basic type check
	dataMap, ok := data.(map[string]interface{})
	if !ok {
		return fmt.Errorf("expected object, got %T", data)
	}

	// Check for required "code" field
	code, ok := dataMap["code"]
	if !ok {
		return fmt.Errorf("missing required field: code")
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
func (s *ImplementationResultSchema) Transform(data interface{}) (interface{}, error) {
	// Extract the code string
	dataMap := data.(map[string]interface{})
	return dataMap["code"], nil
}
