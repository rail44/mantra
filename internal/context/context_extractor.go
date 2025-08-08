package context

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/rail44/mantra/internal/analysis"
)

// GetContextForTarget extracts context for a specific target using go/packages
func (l *PackageLoader) GetContextForTarget(targetPath string, directlyUsedTypes map[string]bool, targetMethodName string) (*RelevantContext, error) {
	if err := l.Load(); err != nil {
		return nil, err
	}

	ctx := &RelevantContext{
		Types:       make(map[string]string),
		Methods:     make(map[string][]analysis.MethodInfo),
		PackageName: l.pkg.Name,
	}

	// Get all types in the package
	allTypes, err := l.GetAllTypes()
	if err != nil {
		return nil, err
	}

	// Add directly used types
	for typeName := range directlyUsedTypes {
		if typeInfo, exists := allTypes[typeName]; exists {
			ctx.Types[typeName] = l.buildCompleteTypeDefinition(typeInfo)
			if len(typeInfo.Methods) > 0 {
				// Filter out the method being implemented to avoid recursive calls
				var filteredMethods []analysis.MethodInfo
				for _, method := range typeInfo.Methods {
					if method.Name != targetMethodName {
						filteredMethods = append(filteredMethods, method)
					}
				}
				if len(filteredMethods) > 0 {
					ctx.Methods[typeName] = filteredMethods
				}
			}
		}
	}

	// Recursively add referenced types (up to 3 levels)
	for i := 0; i < 3; i++ {
		initialCount := len(ctx.Types)
		typesToCheck := make(map[string]string)
		for name, def := range ctx.Types {
			typesToCheck[name] = def
		}

		for _, typeDef := range typesToCheck {
			referencedTypes := analysis.ExtractReferencedTypesFromDefinition(typeDef)
			for refType := range referencedTypes {
				if _, exists := ctx.Types[refType]; !exists {
					if typeInfo, exists := allTypes[refType]; exists {
						ctx.Types[refType] = l.buildCompleteTypeDefinition(typeInfo)
						if len(typeInfo.Methods) > 0 {
							// For referenced types, include all methods (they're not the receiver)
							ctx.Methods[refType] = typeInfo.Methods
						}
					}
				}
			}
		}

		// If no new types were added, stop recursion
		if len(ctx.Types) == initialCount {
			break
		}
	}

	// Extract imports from the first syntax file
	if len(l.pkg.Syntax) > 0 {
		// Find the file matching targetPath
		for _, file := range l.pkg.Syntax {
			pos := l.pkg.Fset.Position(file.Pos())
			if filepath.Base(pos.Filename) == filepath.Base(targetPath) {
				ctx.Imports = ExtractImportInfo(file)
				break
			}
		}
	}

	return ctx, nil
}

// buildCompleteTypeDefinition builds a complete type definition including fields
func (l *PackageLoader) buildCompleteTypeDefinition(typeInfo *TypeInfo) string {
	switch typeInfo.Kind {
	case "struct":
		var builder strings.Builder
		builder.WriteString(fmt.Sprintf("type %s struct {\n", typeInfo.Name))
		for _, field := range typeInfo.Fields {
			// Simplify the field type for readability
			fieldType := l.simplifyFieldTypeName(field.Type)
			builder.WriteString(fmt.Sprintf("    %s %s\n", field.Name, fieldType))
		}
		builder.WriteString("}")
		return builder.String()

	case "interface":
		var builder strings.Builder
		builder.WriteString(fmt.Sprintf("type %s interface {\n", typeInfo.Name))
		for _, method := range typeInfo.Methods {
			builder.WriteString(fmt.Sprintf("    %s\n", method.Signature))
		}
		builder.WriteString("}")
		return builder.String()

	default:
		// For type aliases or basic types
		return typeInfo.Definition
	}
}

// simplifyFieldTypeName simplifies type names for better readability
func (l *PackageLoader) simplifyFieldTypeName(typeName string) string {
	// First apply the general simplification
	simplified := l.simplifyTypeName(typeName)

	// Handle special cases for field types
	// Remove unnecessary package qualifiers for common types
	if strings.Contains(simplified, ".") {
		parts := strings.Split(simplified, ".")
		if len(parts) == 2 {
			pkg := parts[0]
			name := parts[1]

			// Keep standard library package names
			standardPkgs := map[string]bool{
				"time":     true,
				"sync":     true,
				"context":  true,
				"io":       true,
				"fmt":      true,
				"errors":   true,
				"strings":  true,
				"bytes":    true,
				"json":     true,
				"encoding": true,
			}

			if standardPkgs[pkg] {
				return simplified // Keep as is
			}

			// For local packages, just use the type name
			if pkg == l.pkg.Name {
				return name
			}
		}
	}

	return simplified
}

// simplifyTypeName removes package path for types in the same package
func (l *PackageLoader) simplifyTypeName(typeName string) string {
	if l.pkg == nil {
		return typeName
	}

	// Remove the current package path
	pkgPath := l.pkg.PkgPath
	if pkgPath != "" {
		// Replace full package path with just the type name
		// e.g., "github.com/rail44/mantra/examples/simple.cacheItem" -> "cacheItem"
		prefix := pkgPath + "."
		if strings.HasPrefix(typeName, prefix) {
			return strings.TrimPrefix(typeName, prefix)
		}

		// Handle pointer types
		// e.g., "*github.com/rail44/mantra/examples/simple.SimpleCache" -> "*SimpleCache"
		if strings.HasPrefix(typeName, "*"+prefix) {
			return "*" + strings.TrimPrefix(typeName, "*"+prefix)
		}
	}

	// Handle slices
	if strings.HasPrefix(typeName, "[]") {
		return "[]" + l.simplifyTypeName(typeName[2:])
	}

	// Handle maps
	if strings.HasPrefix(typeName, "map[") {
		// This is more complex, but for now keep as is
		return typeName
	}

	return typeName
}

// getFunctionImplementation extracts function body from AST
func (l *PackageLoader) getFunctionImplementation(funcName string) string {
	// Note: This is currently a placeholder
	// Real implementation would need to traverse AST and extract function body
	// For now, we don't provide implementations as they're not needed for AI generation
	return ""
}
