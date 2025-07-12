package ast

import (
	"go/ast"
)

// GetTypeString returns a string representation of an AST type expression
func GetTypeString(expr ast.Expr) string {
	switch t := expr.(type) {
	case *ast.Ident:
		return t.Name
	case *ast.ArrayType:
		return "[]" + GetTypeString(t.Elt)
	case *ast.StarExpr:
		return "*" + GetTypeString(t.X)
	case *ast.SelectorExpr:
		return GetTypeString(t.X) + "." + t.Sel.Name
	case *ast.FuncType:
		return "func" // TODO: More detailed function type representation
	case *ast.InterfaceType:
		return "interface{}"
	case *ast.MapType:
		return "map[" + GetTypeString(t.Key) + "]" + GetTypeString(t.Value)
	case *ast.ChanType:
		return "chan " + GetTypeString(t.Value)
	case *ast.StructType:
		return "struct{}" // TODO: More detailed struct representation
	case *ast.Ellipsis:
		return "..." + GetTypeString(t.Elt)
	default:
		return "unknown"
	}
}
