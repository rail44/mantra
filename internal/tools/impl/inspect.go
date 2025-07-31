package impl

import (
	"context"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"

	"github.com/rail44/mantra/internal/tools"
)

// InspectTool provides detailed information about any Go declaration
type InspectTool struct {
	projectRoot string
	fileCache   map[string]*ast.File
	fset        *token.FileSet
}

// NewInspectTool creates a new inspect tool
func NewInspectTool() *InspectTool {
	// Get current working directory as project root
	cwd, _ := os.Getwd()
	return &InspectTool{
		projectRoot: cwd,
		fileCache:   make(map[string]*ast.File),
		fset:        token.NewFileSet(),
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

	// Find the declaration
	result, err := t.findDeclaration(name)
	if err != nil {
		return &InspectResult{
			Found: false,
			Name:  name,
			Error: fmt.Sprintf("Failed to find declaration: %v", err),
		}, nil
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
	Value       string                 `json:"value,omitempty"`       // For constants only
	Type        string                 `json:"type,omitempty"`        // For variables/constants
	InitPattern string                 `json:"init_pattern,omitempty"` // For variables (e.g., "errors.New")
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
	var foundResult *InspectResult

	// Walk through project files to find the declaration
	err := filepath.Walk(t.projectRoot, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil // Skip files with errors
		}

		// Skip non-Go files and test files
		if !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
			return nil
		}

		// Skip vendor and hidden directories
		if info.IsDir() && (strings.HasPrefix(info.Name(), ".") || info.Name() == "vendor") {
			return filepath.SkipDir
		}

		// Parse file if not cached
		file, err := t.parseFile(path)
		if err != nil {
			return nil // Skip files that can't be parsed
		}

		// Search for the declaration in this file
		if result := t.searchInFile(file, name, path); result != nil {
			foundResult = result
			return filepath.SkipDir // Stop walking once found
		}

		return nil
	})

	if err != nil {
		return nil, err
	}

	if foundResult != nil {
		return foundResult, nil
	}

	return &InspectResult{
		Found: false,
		Name:  name,
		Error: fmt.Sprintf("Declaration '%s' not found", name),
	}, nil
}

func (t *InspectTool) parseFile(path string) (*ast.File, error) {
	// Check cache first
	if file, ok := t.fileCache[path]; ok {
		return file, nil
	}

	// Parse the file
	src, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}

	file, err := parser.ParseFile(t.fset, path, src, parser.ParseComments)
	if err != nil {
		return nil, err
	}

	// Cache the parsed file
	t.fileCache[path] = file
	return file, nil
}

func (t *InspectTool) searchInFile(file *ast.File, name string, path string) *InspectResult {
	var result *InspectResult
	
	// Get relative path for display
	relPath, _ := filepath.Rel(t.projectRoot, path)
	if relPath == "" {
		relPath = path
	}

	// Inspect all declarations
	ast.Inspect(file, func(n ast.Node) bool {
		if result != nil {
			return false // Already found
		}

		switch decl := n.(type) {
		case *ast.GenDecl:
			// Type, const, or var declaration
			for _, spec := range decl.Specs {
				switch s := spec.(type) {
				case *ast.TypeSpec:
					if s.Name.Name == name {
						result = t.extractTypeInfo(s, file.Name.Name, relPath)
						return false
					}
				case *ast.ValueSpec:
					// Const or var
					for _, id := range s.Names {
						if id.Name == name {
							result = t.extractValueInfo(s, decl, id, file.Name.Name, relPath)
							return false
						}
					}
				}
			}
		case *ast.FuncDecl:
			// Function or method
			if decl.Name.Name == name {
				result = t.extractFuncInfo(decl, file.Name.Name, relPath)
				return false
			}
		}
		return true
	})

	return result
}

func (t *InspectTool) extractTypeInfo(spec *ast.TypeSpec, pkg, path string) *InspectResult {
	result := &InspectResult{
		Found:    true,
		Name:     spec.Name.Name,
		Package:  pkg,
		Location: fmt.Sprintf("%s:%d", path, t.fset.Position(spec.Pos()).Line),
	}

	switch typ := spec.Type.(type) {
	case *ast.StructType:
		result.Kind = "struct"
		result.Fields = extractStructFields(typ)
		result.Definition = fmt.Sprintf("type %s struct", spec.Name.Name)
	case *ast.InterfaceType:
		result.Kind = "interface"
		result.Methods = extractInterfaceMethods(typ)
		result.Definition = fmt.Sprintf("type %s interface", spec.Name.Name)
	default:
		result.Kind = "type"
		result.Type = extractTypeString(spec.Type)
		result.Definition = fmt.Sprintf("type %s %s", spec.Name.Name, result.Type)
	}

	return result
}

func (t *InspectTool) extractValueInfo(spec *ast.ValueSpec, decl *ast.GenDecl, id *ast.Ident, pkg, path string) *InspectResult {
	result := &InspectResult{
		Found:    true,
		Name:     id.Name,
		Package:  pkg,
		Location: fmt.Sprintf("%s:%d", path, t.fset.Position(id.Pos()).Line),
	}

	// Determine if it's const or var
	if decl.Tok == token.CONST {
		result.Kind = "const"
	} else {
		result.Kind = "var"
	}

	// Extract type if specified
	if spec.Type != nil {
		result.Type = extractTypeString(spec.Type)
	}

	// Extract value ONLY for constants
	if decl.Tok == token.CONST && len(spec.Values) > 0 {
		// Find the index of this identifier
		idx := -1
		for i, n := range spec.Names {
			if n == id {
				idx = i
				break
			}
		}
		if idx >= 0 && idx < len(spec.Values) {
			// Try to extract the value
			valueStr := extractValueString(spec.Values[idx])
			if valueStr != "" {
				result.Value = valueStr
			}
		}
	}
	
	// For variables, show initialization pattern if it's a common pattern
	if decl.Tok == token.VAR && len(spec.Values) > 0 {
		// Find the index of this identifier
		idx := -1
		for i, n := range spec.Names {
			if n == id {
				idx = i
				break
			}
		}
		if idx >= 0 && idx < len(spec.Values) {
			// Check for common patterns like errors.New
			if call, ok := spec.Values[idx].(*ast.CallExpr); ok {
				if sel, ok := call.Fun.(*ast.SelectorExpr); ok {
					if x, ok := sel.X.(*ast.Ident); ok {
						if x.Name == "errors" && sel.Sel.Name == "New" {
							// This is errors.New(...) pattern
							result.InitPattern = "errors.New"
						}
					}
				}
			}
		}
	}
	// Already handled above

	// Build definition
	var defParts []string
	defParts = append(defParts, result.Kind, id.Name)
	if result.Type != "" {
		defParts = append(defParts, result.Type)
	}
	if result.Value != "" {
		// For constants, show the value
		defParts = append(defParts, "=", result.Value)
	} else if result.InitPattern != "" {
		// For variables, show the initialization pattern
		defParts = append(defParts, "=", result.InitPattern+"(...)")
	}
	result.Definition = strings.Join(defParts, " ")

	return result
}

func (t *InspectTool) extractFuncInfo(decl *ast.FuncDecl, pkg, path string) *InspectResult {
	result := &InspectResult{
		Found:     true,
		Name:      decl.Name.Name,
		Package:   pkg,
		Location:  fmt.Sprintf("%s:%d", path, t.fset.Position(decl.Pos()).Line),
		Signature: buildFunctionSignatureFromDecl(decl),
	}

	if decl.Recv != nil {
		result.Kind = "method"
		// Extract receiver type
		if len(decl.Recv.List) > 0 {
			result.Type = extractTypeString(decl.Recv.List[0].Type)
		}
	} else {
		result.Kind = "func"
	}

	result.Definition = result.Signature
	
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


