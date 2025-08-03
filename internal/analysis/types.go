package analysis

import (
	"go/ast"
	"strings"
)

// ExtractTypeString extracts a string representation of a type from AST
func ExtractTypeString(expr ast.Expr) string {
	switch t := expr.(type) {
	case *ast.Ident:
		return t.Name
	case *ast.StarExpr:
		return "*" + ExtractTypeString(t.X)
	case *ast.ArrayType:
		if t.Len == nil {
			return "[]" + ExtractTypeString(t.Elt)
		}
		return "[]" + ExtractTypeString(t.Elt) // Simplified for now
	case *ast.MapType:
		return "map[" + ExtractTypeString(t.Key) + "]" + ExtractTypeString(t.Value)
	case *ast.SelectorExpr:
		if ident, ok := t.X.(*ast.Ident); ok {
			return ident.Name + "." + t.Sel.Name
		}
		return "qualified.Type"
	case *ast.ChanType:
		return "chan " + ExtractTypeString(t.Value)
	case *ast.InterfaceType:
		return "interface{}"
	case *ast.FuncType:
		return FormatFuncType(t)
	default:
		return "interface{}"
	}
}

// CleanTypeName removes pointers, slices, and other modifiers from type name
func CleanTypeName(typeStr string) string {
	// Remove common prefixes
	typeStr = strings.TrimPrefix(typeStr, "*")
	typeStr = strings.TrimPrefix(typeStr, "[]")
	typeStr = strings.TrimPrefix(typeStr, "chan ")

	// Handle maps - extract value type
	if strings.HasPrefix(typeStr, "map[") {
		if idx := strings.Index(typeStr, "]"); idx > 0 && idx < len(typeStr)-1 {
			return CleanTypeName(typeStr[idx+1:])
		}
	}

	// Skip qualified types (package.Type)
	if strings.Contains(typeStr, ".") {
		return ""
	}

	return typeStr
}

// IsBuiltinType checks if a type name is a Go builtin type
func IsBuiltinType(typeName string) bool {
	builtins := map[string]bool{
		"bool":        true,
		"byte":        true,
		"complex64":   true,
		"complex128":  true,
		"error":       true,
		"float32":     true,
		"float64":     true,
		"int":         true,
		"int8":        true,
		"int16":       true,
		"int32":       true,
		"int64":       true,
		"rune":        true,
		"string":      true,
		"uint":        true,
		"uint8":       true,
		"uint16":      true,
		"uint32":      true,
		"uint64":      true,
		"uintptr":     true,
		"interface{}": true,
	}
	return builtins[typeName]
}
