package analysis

import (
	"go/ast"
	"go/token"
	"strings"
)

// FormatTypeDefinition formats a complete type definition with its name
func FormatTypeDefinition(spec *ast.TypeSpec, fset *token.FileSet) string {
	var result strings.Builder
	result.WriteString("type ")
	result.WriteString(spec.Name.Name)
	result.WriteString(" ")

	switch t := spec.Type.(type) {
	case *ast.StructType:
		result.WriteString(FormatStructType(t))
	case *ast.InterfaceType:
		result.WriteString(FormatInterfaceType(t))
	default:
		result.WriteString("any")
	}

	return result.String()
}

// ExtractReferencedTypesFromDefinition extracts type names from a type definition string
func ExtractReferencedTypesFromDefinition(typeDef string) map[string]bool {
	types := make(map[string]bool)

	// Simple pattern matching for field types
	lines := strings.Split(typeDef, "\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "//") || strings.HasPrefix(line, "type ") {
			continue
		}

		// Look for field definitions: "fieldName TypeName"
		parts := strings.Fields(line)
		if len(parts) >= 2 {
			// Last part is likely the type
			typeName := CleanTypeName(parts[len(parts)-1])
			if typeName != "" && !IsBuiltinType(typeName) && !strings.Contains(typeName, "{") && !strings.Contains(typeName, "}") {
				types[typeName] = true
			}
		}
	}

	return types
}

// ExtractImports extracts imports from the file
func ExtractImports(node *ast.File) []string {
	var imports []string
	for _, imp := range node.Imports {
		if imp.Name != nil {
			// Skip blank imports (they're for generated code, not context)
			if imp.Name.Name == "_" {
				continue
			}
			// Named import: alias "path"
			imports = append(imports, imp.Name.Name+" "+imp.Path.Value)
		} else {
			// Regular import: "path"
			imports = append(imports, imp.Path.Value)
		}
	}
	return imports
}

// ExtractBlankImports extracts blank imports (imports with _) from the file
func ExtractBlankImports(node *ast.File) []string {
	var blankImports []string
	for _, imp := range node.Imports {
		if imp.Name != nil && imp.Name.Name == "_" {
			// Remove quotes from import path
			path := strings.Trim(imp.Path.Value, `"`)
			blankImports = append(blankImports, path)
		}
	}
	return blankImports
}
