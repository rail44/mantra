package analysis

import (
	"fmt"
	"go/ast"
	"strings"
)

// FormatFuncType formats a function type in a readable way
func FormatFuncType(fn *ast.FuncType) string {
	var result strings.Builder
	result.WriteString("(")

	// Parameters
	if fn.Params != nil {
		for i, field := range fn.Params.List {
			if i > 0 {
				result.WriteString(", ")
			}
			result.WriteString(ExtractTypeString(field.Type))
		}
	}

	result.WriteString(")")

	// Results
	if fn.Results != nil && len(fn.Results.List) > 0 {
		result.WriteString(" ")
		if len(fn.Results.List) > 1 {
			result.WriteString("(")
		}
		for i, field := range fn.Results.List {
			if i > 0 {
				result.WriteString(", ")
			}
			result.WriteString(ExtractTypeString(field.Type))
		}
		if len(fn.Results.List) > 1 {
			result.WriteString(")")
		}
	}

	return result.String()
}

// BuildFunctionSignature builds a function signature string
func BuildFunctionSignature(name string, funcType *ast.FuncType) string {
	var parts []string

	// Parameters
	if funcType.Params != nil {
		var params []string
		for _, param := range funcType.Params.List {
			paramType := ExtractTypeString(param.Type)
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
			results = append(results, ExtractTypeString(result.Type))
		}

		if len(results) == 1 {
			parts = append(parts, results[0])
		} else if len(results) > 1 {
			parts = append(parts, fmt.Sprintf("(%s)", strings.Join(results, ", ")))
		}
	}

	return fmt.Sprintf("%s%s", name, strings.Join(parts, " "))
}

// BuildFunctionSignatureFromDecl builds a function signature from FuncDecl
func BuildFunctionSignatureFromDecl(decl *ast.FuncDecl) string {
	return BuildFunctionSignature(decl.Name.Name, decl.Type)
}
