package impl

import (
	"context"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"path/filepath"
	"strings"

	"github.com/rail44/mantra/internal/tools"
)

// SearchTool searches for declarations using pattern matching
type SearchTool struct {
	projectRoot string
	fileCache   map[string]*ast.File
	fset        *token.FileSet
}

// NewSearchTool creates a new search tool
func NewSearchTool(projectRoot string) *SearchTool {
	return &SearchTool{
		projectRoot: projectRoot,
		fileCache:   make(map[string]*ast.File),
		fset:        token.NewFileSet(),
	}
}

// Name returns the tool name
func (t *SearchTool) Name() string {
	return "search"
}

// Description returns what this tool does
func (t *SearchTool) Description() string {
	return "Search for Go declarations using pattern matching (supports wildcards)"
}

// ParametersSchema returns the JSON Schema for parameters
func (t *SearchTool) ParametersSchema() json.RawMessage {
	return json.RawMessage(`{
		"type": "object",
		"properties": {
			"pattern": {
				"type": "string",
				"description": "Search pattern (supports * wildcard, e.g., '*Repository', 'Create*')"
			},
			"kind": {
				"type": "string",
				"enum": ["all", "struct", "interface", "func", "method", "const", "var", "type"],
				"default": "all",
				"description": "Type of declarations to search"
			},
			"limit": {
				"type": "integer",
				"default": 10,
				"description": "Maximum number of results"
			}
		},
		"required": ["pattern"]
	}`)
}

// Execute runs the search tool
func (t *SearchTool) Execute(ctx context.Context, params map[string]interface{}) (interface{}, error) {
	// Extract parameters
	pattern, ok := params["pattern"].(string)
	if !ok {
		return nil, &tools.ToolError{
			Code:    "invalid_params",
			Message: "Parameter 'pattern' is required and must be a string",
		}
	}

	kind := "all"
	if k, ok := params["kind"].(string); ok {
		kind = k
	}

	limit := 10
	if l, ok := params["limit"].(float64); ok {
		limit = int(l)
	}

	// Perform search
	results, err := t.search(ctx, pattern, kind, limit)
	if err != nil {
		return nil, err
	}

	return SearchResults{
		Pattern: pattern,
		Kind:    kind,
		Results: results,
		Count:   len(results),
	}, nil
}

// SearchResults represents the search results
type SearchResults struct {
	Pattern string         `json:"pattern"`
	Kind    string         `json:"kind"`
	Results []SearchResult `json:"results"`
	Count   int            `json:"count"`
}

// SearchResult represents a single search result
type SearchResult struct {
	Name      string `json:"name"`
	Kind      string `json:"kind"`
	Package   string `json:"package"`
	Location  string `json:"location"`
	Signature string `json:"signature,omitempty"` // For functions/methods
}

func (t *SearchTool) search(ctx context.Context, pattern, kind string, limit int) ([]SearchResult, error) {
	var results []SearchResult

	// Walk through Go files in the project
	err := filepath.WalkDir(t.projectRoot, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}

		// Skip non-Go files
		if d.IsDir() || !strings.HasSuffix(path, ".go") {
			return nil
		}

		// Skip vendor and hidden directories
		if strings.Contains(path, "/vendor/") || strings.Contains(path, "/.") {
			return nil
		}

		// Skip test files (optional)
		if strings.HasSuffix(path, "_test.go") {
			return nil
		}

		// Parse file
		file, err := t.parseFile(path)
		if err != nil {
			// Skip files with parse errors
			return nil
		}

		// Search in file
		fileResults := t.searchInFile(file, path, pattern, kind)
		results = append(results, fileResults...)

		// Check limit
		if len(results) >= limit {
			results = results[:limit]
			return filepath.SkipAll
		}

		// Check context cancellation
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}

		return nil
	})

	return results, err
}

func (t *SearchTool) parseFile(path string) (*ast.File, error) {
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

func (t *SearchTool) searchInFile(file *ast.File, path, pattern, kind string) []SearchResult {
	var results []SearchResult
	relPath, _ := filepath.Rel(t.projectRoot, path)

	// Search in declarations
	for _, decl := range file.Decls {
		switch d := decl.(type) {
		case *ast.GenDecl:
			// Handle type, const, var declarations
			for _, spec := range d.Specs {
				result := t.checkGenSpec(spec, d.Tok, file.Name.Name, relPath, pattern, kind)
				if result != nil {
					results = append(results, *result)
				}
			}

		case *ast.FuncDecl:
			// Handle function/method declarations
			if kind == "all" || kind == "func" || (kind == "method" && d.Recv != nil) {
				if matchesPattern(d.Name.Name, pattern) {
					result := SearchResult{
						Name:      d.Name.Name,
						Kind:      "func",
						Package:   file.Name.Name,
						Location:  fmt.Sprintf("%s:%d", relPath, t.fset.Position(d.Pos()).Line),
						Signature: buildFunctionSignatureFromDecl(d),
					}
					if d.Recv != nil {
						result.Kind = "method"
					}
					results = append(results, result)
				}
			}
		}
	}

	return results
}

func (t *SearchTool) checkGenSpec(spec ast.Spec, tok token.Token, pkg, path, pattern, kind string) *SearchResult {
	switch s := spec.(type) {
	case *ast.TypeSpec:
		// Type declaration
		if kind == "all" || kind == "type" || kind == "struct" || kind == "interface" {
			if matchesPattern(s.Name.Name, pattern) {
				result := &SearchResult{
					Name:     s.Name.Name,
					Kind:     "type",
					Package:  pkg,
					Location: fmt.Sprintf("%s:%d", path, t.fset.Position(s.Pos()).Line),
				}

				// Determine specific type kind
				switch s.Type.(type) {
				case *ast.StructType:
					result.Kind = "struct"
				case *ast.InterfaceType:
					result.Kind = "interface"
				}

				// Check if kind matches
				if kind == "all" || kind == result.Kind {
					return result
				}
			}
		}

	case *ast.ValueSpec:
		// Const or var declaration
		declKind := "var"
		if tok == token.CONST {
			declKind = "const"
		}

		if kind == "all" || kind == declKind {
			for _, name := range s.Names {
				if matchesPattern(name.Name, pattern) {
					return &SearchResult{
						Name:     name.Name,
						Kind:     declKind,
						Package:  pkg,
						Location: fmt.Sprintf("%s:%d", path, t.fset.Position(name.Pos()).Line),
					}
				}
			}
		}
	}

	return nil
}

