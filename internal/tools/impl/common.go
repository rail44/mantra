package impl

import (
	"fmt"
	"go/ast"
	"strings"
)

// extractTypeString extracts a string representation of a type from AST
func extractTypeString(expr ast.Expr) string {
	switch t := expr.(type) {
	case *ast.Ident:
		return t.Name
	case *ast.StarExpr:
		return "*" + extractTypeString(t.X)
	case *ast.ArrayType:
		return "[]" + extractTypeString(t.Elt)
	case *ast.SelectorExpr:
		return extractTypeString(t.X) + "." + t.Sel.Name
	case *ast.InterfaceType:
		return "interface{}"
	case *ast.MapType:
		return "map[" + extractTypeString(t.Key) + "]" + extractTypeString(t.Value)
	case *ast.ChanType:
		return "chan " + extractTypeString(t.Value)
	case *ast.FuncType:
		return "func" // TODO: Full function type
	default:
		return "unknown"
	}
}

// matchesPattern checks if a name matches a wildcard pattern
func matchesPattern(name, pattern string) bool {
	// Handle wildcards
	if pattern == "*" {
		return true
	}

	// Convert pattern to simple regex-like matching
	if strings.HasPrefix(pattern, "*") && strings.HasSuffix(pattern, "*") {
		// *abc* - contains
		substr := pattern[1 : len(pattern)-1]
		return strings.Contains(name, substr)
	} else if strings.HasPrefix(pattern, "*") {
		// *abc - ends with
		suffix := pattern[1:]
		return strings.HasSuffix(name, suffix)
	} else if strings.HasSuffix(pattern, "*") {
		// abc* - starts with
		prefix := pattern[:len(pattern)-1]
		return strings.HasPrefix(name, prefix)
	}

	// Exact match
	return name == pattern
}

// buildFunctionSignature builds a function signature string
func buildFunctionSignature(name string, funcType *ast.FuncType) string {
	var parts []string

	// Parameters
	if funcType.Params != nil {
		var params []string
		for _, param := range funcType.Params.List {
			paramType := extractTypeString(param.Type)
			if len(param.Names) == 0 {
				params = append(params, paramType)
			} else {
				for _, paramName := range param.Names {
					params = append(params, fmt.Sprintf("%s %s", paramName.Name, paramType))
				}
			}
		}
		parts = append(parts, fmt.Sprintf("(%s)", strings.Join(params, ", ")))
	}

	// Results
	if funcType.Results != nil {
		var results []string
		for _, result := range funcType.Results.List {
			results = append(results, extractTypeString(result.Type))
		}

		if len(results) == 1 {
			parts = append(parts, results[0])
		} else if len(results) > 1 {
			parts = append(parts, fmt.Sprintf("(%s)", strings.Join(results, ", ")))
		}
	}

	return fmt.Sprintf("%s%s", name, strings.Join(parts, " "))
}

// buildFunctionSignatureFromDecl builds a function signature from FuncDecl
func buildFunctionSignatureFromDecl(decl *ast.FuncDecl) string {
	return buildFunctionSignature(decl.Name.Name, decl.Type)
}
