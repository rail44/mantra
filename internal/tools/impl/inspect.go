package impl

import (
	"context"
	"encoding/json"
	"fmt"
	"os"

	pkgcontext "github.com/rail44/mantra/internal/context"
	"github.com/rail44/mantra/internal/tools"
)

// InspectTool uses go/packages for accurate type information
type InspectTool struct {
	loader *pkgcontext.PackageLoader
}

// NewInspectTool creates a new inspect tool using go/packages
func NewInspectTool(packagePath string) *InspectTool {
	if packagePath == "" {
		packagePath, _ = os.Getwd()
	}
	return &InspectTool{
		loader: pkgcontext.NewPackageLoader(packagePath),
	}
}

// Name returns the tool name
func (t *InspectTool) Name() string {
	return "inspect"
}

// Description returns what this tool does
func (t *InspectTool) Description() string {
	return "Get detailed information about Go declarations from current package or imported packages (e.g., 'SimpleCache', 'time.Time')"
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

	// Try to get the declaration using the loader
	decl, err := t.loader.GetDeclaration(name)
	if err != nil {
		// Return JSON-serializable map for not found
		return map[string]interface{}{
			"found": false,
			"name":  name,
			"kind":  "not_found",
			"error": fmt.Sprintf("Declaration '%s' not found", name),
		}, nil
	}

	// Convert Declaration to JSON-serializable map
	return convertDeclarationToMap(decl), nil
}

// convertDeclarationToMap converts a Declaration to a JSON-serializable map
func convertDeclarationToMap(decl pkgcontext.Declaration) map[string]interface{} {
	result := map[string]interface{}{
		"found":   decl.IsFound(),
		"name":    decl.GetName(),
		"kind":    decl.GetKind(),
		"package": decl.GetPackage(),
	}

	// Add type-specific fields based on the concrete type
	switch d := decl.(type) {
	case *pkgcontext.StructDeclaration:
		result["definition"] = d.Definition
		if len(d.Fields) > 0 {
			result["fields"] = d.Fields
		}
		if len(d.Methods) > 0 {
			result["methods"] = d.Methods
		}

	case *pkgcontext.InterfaceDeclaration:
		result["definition"] = d.Definition
		if len(d.Methods) > 0 {
			result["methods"] = d.Methods
		}

	case *pkgcontext.FunctionDeclaration:
		result["signature"] = d.Signature
		if d.Receiver != "" {
			result["receiver"] = d.Receiver
		}
		if d.Implementation != "" {
			result["implementation"] = d.Implementation
		}
		if d.Doc != "" {
			result["doc"] = d.Doc
		}

	case *pkgcontext.ConstantDeclaration:
		result["type"] = d.Type
		result["value"] = d.Value

	case *pkgcontext.VariableDeclaration:
		result["type"] = d.Type
		if d.InitPattern != "" {
			result["init_pattern"] = d.InitPattern
		}

	case *pkgcontext.TypeAliasDeclaration:
		result["definition"] = d.Definition
		result["type"] = d.Type
		if len(d.Methods) > 0 {
			result["methods"] = d.Methods
		}
	}

	return result
}
