package context

import (
	"go/ast"
	"strings"
)

// ImportInfo represents information about a single import
type ImportInfo struct {
	// Path is the import path (e.g., "fmt", "github.com/google/uuid")
	Path string

	// Alias is the import alias if specified (e.g., "u" for u "github.com/google/uuid")
	// Empty string means no alias (use default package name)
	// "_" means blank import (available but not directly used)
	Alias string

	// IsBlank indicates if this is a blank import (alias == "_")
	IsBlank bool
}

// GetIdentifier returns the identifier to use for this import in code
// For example: "fmt" for standard import, "u" for aliased import
func (i *ImportInfo) GetIdentifier() string {
	if i.Alias != "" && i.Alias != "_" {
		return i.Alias
	}
	// Extract package name from path (last segment)
	// This is a simplified version - real implementation might need more logic
	if idx := lastIndexByte(i.Path, '/'); idx >= 0 {
		return i.Path[idx+1:]
	}
	return i.Path
}

// lastIndexByte finds the last occurrence of a byte in a string
func lastIndexByte(s string, c byte) int {
	for i := len(s) - 1; i >= 0; i-- {
		if s[i] == c {
			return i
		}
	}
	return -1
}

// ExtractImportInfo extracts structured import information from AST
func ExtractImportInfo(node *ast.File) []*ImportInfo {
	var imports []*ImportInfo

	for _, imp := range node.Imports {
		info := &ImportInfo{
			Path: strings.Trim(imp.Path.Value, `"`),
		}

		if imp.Name != nil {
			info.Alias = imp.Name.Name
			info.IsBlank = (imp.Name.Name == "_")
		}

		imports = append(imports, info)
	}

	return imports
}
