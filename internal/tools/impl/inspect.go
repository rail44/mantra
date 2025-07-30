package impl

import (
	"context"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/token"

	"github.com/rail44/mantra/internal/tools"
)

// InspectTool provides detailed information about any Go declaration
type InspectTool struct {
	// Map of parsed files for caching
	fileCache map[string]*ast.File
	fset      *token.FileSet
}

// NewInspectTool creates a new inspect tool
func NewInspectTool() *InspectTool {
	return &InspectTool{
		fileCache: make(map[string]*ast.File),
		fset:      token.NewFileSet(),
	}
}

// Name returns the tool name
func (t *InspectTool) Name() string {
	return "inspect"
}

// Description returns what this tool does
func (t *InspectTool) Description() string {
	return "Get detailed information about any Go declaration (type, function, const, var)"
}

// ParametersSchema returns the JSON Schema for parameters
func (t *InspectTool) ParametersSchema() json.RawMessage {
	return json.RawMessage(`{
		"type": "object",
		"properties": {
			"name": {
				"type": "string",
				"description": "Name of the declaration to inspect"
			}
		},
		"required": ["name"]
	}`)
}

// Execute runs the inspect tool
func (t *InspectTool) Execute(ctx context.Context, params map[string]interface{}) (interface{}, error) {
	// Extract parameters
	name, ok := params["name"].(string)
	if !ok {
		return nil, &tools.ToolError{
			Code:    "invalid_params",
			Message: "Parameter 'name' is required and must be a string",
		}
	}

	// TODO: Search through project files to find the declaration
	// For now, return a structured response format
	result := InspectResult{
		Found: false,
		Name:  name,
		Error: "Implementation pending - need to integrate with AST parser",
	}

	return result, nil
}

// InspectResult represents the result of inspecting a declaration
type InspectResult struct {
	Found       bool                   `json:"found"`
	Name        string                 `json:"name"`
	Kind        string                 `json:"kind,omitempty"` // "struct", "interface", "func", etc.
	Package     string                 `json:"package,omitempty"`
	Definition  string                 `json:"definition,omitempty"`
	Fields      []FieldInfo            `json:"fields,omitempty"`      // For structs
	Methods     []MethodInfo           `json:"methods,omitempty"`     // For types with methods
	Signature   string                 `json:"signature,omitempty"`   // For functions/methods
	Value       string                 `json:"value,omitempty"`       // For constants
	Type        string                 `json:"type,omitempty"`        // For variables/constants
	Location    string                 `json:"location,omitempty"`    // File and line number
	Error       string                 `json:"error,omitempty"`
}

// FieldInfo represents information about a struct field
type FieldInfo struct {
	Name string `json:"name"`
	Type string `json:"type"`
	Tag  string `json:"tag,omitempty"`
}

// MethodInfo represents information about a method
type MethodInfo struct {
	Name      string `json:"name"`
	Signature string `json:"signature"`
	Receiver  string `json:"receiver,omitempty"`
}

// Helper functions for AST processing

func (t *InspectTool) findDeclaration(name string) (*InspectResult, error) {
	// This is a placeholder for the actual implementation
	// It should:
	// 1. Walk through project files
	// 2. Parse AST if not cached
	// 3. Find the declaration by name
	// 4. Extract relevant information based on declaration type
	
	return nil, fmt.Errorf("not implemented")
}

func extractTypeInfo(spec *ast.TypeSpec, fset *token.FileSet) *InspectResult {
	result := &InspectResult{
		Found: true,
		Name:  spec.Name.Name,
	}

	switch t := spec.Type.(type) {
	case *ast.StructType:
		result.Kind = "struct"
		result.Fields = extractStructFields(t)
	case *ast.InterfaceType:
		result.Kind = "interface"
		result.Methods = extractInterfaceMethods(t)
	default:
		result.Kind = "type"
	}

	return result
}

func extractStructFields(s *ast.StructType) []FieldInfo {
	var fields []FieldInfo
	
	if s.Fields == nil {
		return fields
	}

	for _, field := range s.Fields.List {
		fieldType := extractTypeString(field.Type)
		
		if len(field.Names) == 0 {
			// Embedded field
			fields = append(fields, FieldInfo{
				Name: fieldType,
				Type: fieldType,
			})
		} else {
			// Named fields
			for _, name := range field.Names {
				fieldInfo := FieldInfo{
					Name: name.Name,
					Type: fieldType,
				}
				
				// Extract tag if present
				if field.Tag != nil {
					fieldInfo.Tag = field.Tag.Value
				}
				
				fields = append(fields, fieldInfo)
			}
		}
	}
	
	return fields
}

func extractInterfaceMethods(i *ast.InterfaceType) []MethodInfo {
	var methods []MethodInfo
	
	if i.Methods == nil {
		return methods
	}

	for _, method := range i.Methods.List {
		if len(method.Names) > 0 {
			// Method signature
			if funcType, ok := method.Type.(*ast.FuncType); ok {
				sig := buildFunctionSignature(method.Names[0].Name, funcType)
				methods = append(methods, MethodInfo{
					Name:      method.Names[0].Name,
					Signature: sig,
				})
			}
		}
		// TODO: Handle embedded interfaces
	}
	
	return methods
}


