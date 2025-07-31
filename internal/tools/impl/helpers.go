package impl

import (
	"fmt"
	"go/ast"
	"go/token"
	"strings"
)

// extractValueString extracts a string representation of a value expression
func extractValueString(expr ast.Expr) string {
	switch e := expr.(type) {
	case *ast.BasicLit:
		// String, number, etc.
		return e.Value
	case *ast.Ident:
		// Identifier like true, false, nil
		return e.Name
	case *ast.CallExpr:
		// Function call like errors.New("...")
		return extractCallString(e)
	case *ast.UnaryExpr:
		// Unary expressions like -1
		return fmt.Sprintf("%s%s", e.Op.String(), extractValueString(e.X))
	case *ast.BinaryExpr:
		// Binary expressions
		return fmt.Sprintf("%s %s %s", extractValueString(e.X), e.Op.String(), extractValueString(e.Y))
	}
	return ""
}

// extractCallString extracts a string representation of a function call
func extractCallString(call *ast.CallExpr) string {
	var parts []string
	
	// Get function name
	switch fn := call.Fun.(type) {
	case *ast.Ident:
		parts = append(parts, fn.Name)
	case *ast.SelectorExpr:
		if x, ok := fn.X.(*ast.Ident); ok {
			parts = append(parts, fmt.Sprintf("%s.%s", x.Name, fn.Sel.Name))
		}
	}
	
	// Get arguments
	var args []string
	for _, arg := range call.Args {
		if lit, ok := arg.(*ast.BasicLit); ok && lit.Kind == token.STRING {
			args = append(args, lit.Value)
		} else {
			args = append(args, "...")
		}
	}
	
	if len(parts) > 0 {
		return fmt.Sprintf("%s(%s)", parts[0], strings.Join(args, ", "))
	}
	return ""
}

// Note: extractTypeString is already defined in common.go