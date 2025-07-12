package imports

import (
	"go/ast"
	"go/parser"
	"go/token"
	"strings"
)

// StandardPackages contains commonly used Go standard library packages
var StandardPackages = map[string]string{
	"strings":  "strings",
	"unicode":  "unicode",
	"fmt":      "fmt",
	"errors":   "errors",
	"time":     "time",
	"context":  "context",
	"io":       "io",
	"os":       "os",
	"path":     "path",
	"filepath": "path/filepath",
	"bytes":    "bytes",
	"bufio":    "bufio",
	"math":     "math",
	"sort":     "sort",
	"sync":     "sync",
	"strconv":  "strconv",
	"regexp":   "regexp",
	"json":     "encoding/json",
	"base64":   "encoding/base64",
	"hex":      "encoding/hex",
	"http":     "net/http",
	"url":      "net/url",
}

// AnalyzeRequiredImports analyzes code and returns required import paths
func AnalyzeRequiredImports(code string) []string {
	// Create a minimal Go file to parse the code
	fileContent := `package temp
func temp() {
` + code + `
}`

	fset := token.NewFileSet()
	node, err := parser.ParseFile(fset, "temp.go", fileContent, parser.ParseComments)
	if err != nil {
		// If parsing fails, try simple string matching
		return analyzeByStringMatching(code)
	}

	requiredImports := make(map[string]bool)
	
	// Walk the AST to find package references
	ast.Inspect(node, func(n ast.Node) bool {
		switch x := n.(type) {
		case *ast.SelectorExpr:
			// Handle package.Function calls
			if ident, ok := x.X.(*ast.Ident); ok {
				pkgName := ident.Name
				if importPath, exists := StandardPackages[pkgName]; exists {
					requiredImports[importPath] = true
				}
			}
		case *ast.CallExpr:
			// Check for functions that might need imports
			if ident, ok := x.Fun.(*ast.Ident); ok {
				checkFunctionImport(ident.Name, requiredImports)
			}
		}
		return true
	})

	// Convert map to slice
	var imports []string
	for imp := range requiredImports {
		imports = append(imports, imp)
	}
	
	return imports
}

// analyzeByStringMatching uses simple string matching when AST parsing fails
func analyzeByStringMatching(code string) []string {
	requiredImports := make(map[string]bool)
	
	// Check for package.Function patterns
	for pkgName, importPath := range StandardPackages {
		if strings.Contains(code, pkgName+".") {
			requiredImports[importPath] = true
		}
	}
	
	// Check for specific functions that indicate package usage
	functionIndicators := map[string]string{
		"Sprintf":    "fmt",
		"Printf":     "fmt",
		"Println":    "fmt",
		"Errorf":     "fmt",
		"New":        "errors", // errors.New
		"Now":        "time",   // time.Now
		"Sleep":      "time",   // time.Sleep
		"Background": "context", // context.Background
		"TODO":       "context", // context.TODO
	}
	
	for fn, importPath := range functionIndicators {
		if strings.Contains(code, fn+"(") {
			requiredImports[importPath] = true
		}
	}
	
	// Convert map to slice
	var imports []string
	for imp := range requiredImports {
		imports = append(imports, imp)
	}
	
	return imports
}

// checkFunctionImport checks if a function name indicates a required import
func checkFunctionImport(funcName string, requiredImports map[string]bool) {
	// Map of function names to their import paths
	functionImports := map[string]string{
		"Sprintf":       "fmt",
		"Printf":        "fmt",
		"Println":       "fmt",
		"Errorf":        "fmt",
		"New":           "errors",
		"Unwrap":        "errors",
		"Is":            "errors",
		"As":            "errors",
		"MarshalJSON":   "encoding/json",
		"UnmarshalJSON": "encoding/json",
	}
	
	if importPath, exists := functionImports[funcName]; exists {
		requiredImports[importPath] = true
	}
}

// MergeImports merges new imports with existing imports, avoiding duplicates
func MergeImports(existingImports []string, newImports []string) []string {
	importSet := make(map[string]bool)
	
	// Add existing imports
	for _, imp := range existingImports {
		importSet[imp] = true
	}
	
	// Add new imports
	for _, imp := range newImports {
		importSet[imp] = true
	}
	
	// Convert back to slice
	var merged []string
	for imp := range importSet {
		merged = append(merged, imp)
	}
	
	// Sort for consistency
	sortImports(merged)
	
	return merged
}

// sortImports sorts imports according to Go conventions
func sortImports(imports []string) {
	// Simple alphabetical sort for now
	// In a real implementation, we'd group standard library, external, and local imports
	for i := 0; i < len(imports); i++ {
		for j := i + 1; j < len(imports); j++ {
			if imports[i] > imports[j] {
				imports[i], imports[j] = imports[j], imports[i]
			}
		}
	}
}