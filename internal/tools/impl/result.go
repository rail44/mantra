package impl

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/rail44/mantra/internal/tools/schemas"
)

// ResultTool is a special tool that marks the end of a phase and returns structured data
type ResultTool struct {
	phaseName string
	schema    schemas.ResultSchema
	onResult  func(interface{}) error
}

// NewResultTool creates a new result tool for a specific phase
func NewResultTool(phaseName string, schema schemas.ResultSchema, onResult func(interface{}) error) *ResultTool {
	return &ResultTool{
		phaseName: phaseName,
		schema:    schema,
		onResult:  onResult,
	}
}

// Name returns the tool name
func (t *ResultTool) Name() string {
	return "result"
}

// Description returns what this tool does
func (t *ResultTool) Description() string {
	return fmt.Sprintf("Submit the final result for %s phase and complete the phase", t.phaseName)
}

// ParametersSchema returns the JSON Schema for parameters
func (t *ResultTool) ParametersSchema() json.RawMessage {
	return t.schema.GetSchema()
}

// Execute runs the result tool
func (t *ResultTool) Execute(ctx context.Context, params map[string]interface{}) (interface{}, error) {
	// 1. Validate the parameters against the schema
	if err := t.schema.Validate(params); err != nil {
		return nil, fmt.Errorf("validation failed: %w", err)
	}

	// 2. Transform the data if needed
	transformed, err := t.schema.Transform(params)
	if err != nil {
		return nil, fmt.Errorf("transformation failed: %w", err)
	}

	// 3. Call the callback to store the result
	if err := t.onResult(transformed); err != nil {
		return nil, fmt.Errorf("failed to store result: %w", err)
	}

	// 4. Return success message
	return map[string]interface{}{
		"status":  "success",
		"message": fmt.Sprintf("%s phase completed successfully", t.phaseName),
	}, nil
}

// IsTerminal returns true as this tool ends the current phase
func (t *ResultTool) IsTerminal() bool {
	return true
}
