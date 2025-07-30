package impl

import (
	"context"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/format"
	"go/parser"
	"go/token"
	"path/filepath"
	"strings"

	"github.com/rail44/mantra/internal/tools"
)

// ReadBodyTool reads the implementation of functions and methods
type ReadBodyTool struct {
	projectRoot string
	fileCache   map[string]*ast.File
	fset        *token.FileSet
}

// NewReadBodyTool creates a new read_body tool
func NewReadBodyTool(projectRoot string) *ReadBodyTool {
	return &ReadBodyTool{
		projectRoot: projectRoot,
		fileCache:   make(map[string]*ast.File),
		fset:        token.NewFileSet(),
	}
}

// Name returns the tool name
func (t *ReadBodyTool) Name() string {
	return "read_body"
}

// Description returns what this tool does
func (t *ReadBodyTool) Description() string {
	return "Read the implementation body of a function or method"
}

// ParametersSchema returns the JSON Schema for parameters
func (t *ReadBodyTool) ParametersSchema() json.RawMessage {
	return json.RawMessage(`{
		"type": "object",
		"properties": {
			"name": {
				"type": "string",
				"description": "Function/method name (e.g., 'CreateUser' or 'UserService.CreateUser')"
			}
		},
		"required": ["name"]
	}`)
}

// Execute runs the read_body tool
func (t *ReadBodyTool) Execute(ctx context.Context, params map[string]interface{}) (interface{}, error) {
	// Extract parameters
	name, ok := params["name"].(string)
	if !ok {
		return nil, &tools.ToolError{
			Code:    "invalid_params",
			Message: "Parameter 'name' is required and must be a string",
		}
	}

	// Parse the name (could be "FuncName" or "ReceiverType.MethodName")
	var funcName, receiverType string
	parts := strings.Split(name, ".")
	if len(parts) == 2 {
		receiverType = parts[0]
		funcName = parts[1]
	} else {
		funcName = name
	}

	// Search for the function
	result, err := t.findFunction(ctx, funcName, receiverType)
	if err != nil {
		return nil, err
	}

	if result == nil {
		return ReadBodyResult{
			Found: false,
			Name:  name,
			Error: fmt.Sprintf("Function %q not found", name),
		}, nil
	}

	return result, nil
}

// ReadBodyResult represents the result of reading a function body
type ReadBodyResult struct {
	Found          bool     `json:"found"`
	Name           string   `json:"name"`
	Signature      string   `json:"signature"`
	Implementation string   `json:"implementation"`
	ImportsUsed    []string `json:"imports_used,omitempty"`
	Calls          []string `json:"calls,omitempty"`
	Location       string   `json:"location,omitempty"`
	Error          string   `json:"error,omitempty"`
}

func (t *ReadBodyTool) findFunction(ctx context.Context, funcName, receiverType string) (*ReadBodyResult, error) {
	var searchErr error

	// Search through project files
	var result *ReadBodyResult
	searchTool := NewSearchTool(t.projectRoot)
	
	// Use search tool to find the function
	searchParams := map[string]interface{}{
		"pattern": funcName,
		"kind":    "func",
	}
	if receiverType != "" {
		searchParams["kind"] = "method"
	}

	searchResults, err := searchTool.Execute(ctx, searchParams)
	if err != nil {
		return nil, err
	}

	// Look through search results
	if sr, ok := searchResults.(SearchResults); ok {
		for _, res := range sr.Results {
			// Parse the file to get the function body
			filePath := strings.Split(res.Location, ":")[0]
			fullPath := filepath.Join(t.projectRoot, filePath)
			
			file, err := t.parseFile(fullPath)
			if err != nil {
				continue
			}

			// Find the function in the AST
			ast.Inspect(file, func(n ast.Node) bool {
				if fn, ok := n.(*ast.FuncDecl); ok && fn.Name.Name == funcName {
					// Check receiver if specified
					if receiverType != "" {
						if fn.Recv == nil || len(fn.Recv.List) == 0 {
							return true
						}
						recvType := extractTypeString(fn.Recv.List[0].Type)
						if !strings.Contains(recvType, receiverType) {
							return true
						}
					} else if fn.Recv != nil {
						// Skip methods when looking for functions
						return true
					}

					// Found the function!
					result = t.extractFunctionBody(fn, file, fullPath)
					return false
				}
				return true
			})

			if result != nil {
				break
			}
		}
	}

	return result, searchErr
}

func (t *ReadBodyTool) parseFile(path string) (*ast.File, error) {
	// Check cache
	if file, ok := t.fileCache[path]; ok {
		return file, nil
	}

	// Parse file
	file, err := parser.ParseFile(t.fset, path, nil, parser.ParseComments)
	if err != nil {
		return nil, err
	}

	// Cache result
	t.fileCache[path] = file
	return file, nil
}

func (t *ReadBodyTool) extractFunctionBody(fn *ast.FuncDecl, file *ast.File, path string) *ReadBodyResult {
	result := &ReadBodyResult{
		Found:     true,
		Name:      fn.Name.Name,
		Signature: buildFunctionSignatureFromDecl(fn),
		Location:  fmt.Sprintf("%s:%d", path, t.fset.Position(fn.Pos()).Line),
	}

	// Extract body implementation
	if fn.Body != nil {
		bodyStr := t.formatBody(fn.Body)
		result.Implementation = bodyStr

		// Extract function calls
		result.Calls = t.extractFunctionCalls(fn.Body)

		// Extract imports used (simplified - looks for package references)
		result.ImportsUsed = t.extractImportsUsed(fn.Body, file)
	}

	return result
}

func (t *ReadBodyTool) formatBody(body *ast.BlockStmt) string {
	if body == nil || len(body.List) == 0 {
		return ""
	}

	// Format the body statements
	var buf strings.Builder
	for i, stmt := range body.List {
		if i > 0 {
			buf.WriteString("\n")
		}
		
		// Format each statement
		stmtStr := t.formatStatement(stmt)
		buf.WriteString(stmtStr)
	}

	return buf.String()
}

func (t *ReadBodyTool) formatStatement(stmt ast.Stmt) string {
	// Use go/format to format the statement
	var buf strings.Builder
	err := format.Node(&buf, t.fset, stmt)
	if err != nil {
		return fmt.Sprintf("// Failed to format: %v", err)
	}
	return buf.String()
}

func (t *ReadBodyTool) extractFunctionCalls(body *ast.BlockStmt) []string {
	var calls []string
	seen := make(map[string]bool)

	ast.Inspect(body, func(n ast.Node) bool {
		if call, ok := n.(*ast.CallExpr); ok {
			callStr := t.extractCallName(call)
			if callStr != "" && !seen[callStr] {
				seen[callStr] = true
				calls = append(calls, callStr)
			}
		}
		return true
	})

	return calls
}

func (t *ReadBodyTool) extractCallName(call *ast.CallExpr) string {
	switch fun := call.Fun.(type) {
	case *ast.Ident:
		return fun.Name
	case *ast.SelectorExpr:
		if x, ok := fun.X.(*ast.Ident); ok {
			return x.Name + "." + fun.Sel.Name
		}
	}
	return ""
}

func (t *ReadBodyTool) extractImportsUsed(body *ast.BlockStmt, file *ast.File) []string {
	imports := make(map[string]bool)

	// Extract package names from selector expressions
	ast.Inspect(body, func(n ast.Node) bool {
		if sel, ok := n.(*ast.SelectorExpr); ok {
			if x, ok := sel.X.(*ast.Ident); ok {
				// Check if it's a package reference
				if x.Obj == nil { // Package references have nil Obj
					imports[x.Name] = true
				}
			}
		}
		return true
	})

	// Convert to slice
	var result []string
	for imp := range imports {
		result = append(result, imp)
	}

	return result
}