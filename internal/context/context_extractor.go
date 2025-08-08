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

	// Extract imports from the target file first (needed for type simplification)
	var targetImports []*ImportInfo
	if len(l.pkg.Syntax) > 0 {
		// Find the file matching targetPath
		for _, file := range l.pkg.Syntax {
			pos := l.pkg.Fset.Position(file.Pos())
			if filepath.Base(pos.Filename) == filepath.Base(targetPath) {
				targetImports = ExtractImportInfo(file)
				ctx.Imports = targetImports
				break
			}
		}
	}

	// Store imports for use in type simplification
	l.targetImports = targetImports

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
	// Handle map types specially
	if strings.HasPrefix(typeName, "map[") {
		// Find the closing bracket for the key type
		bracketPos := strings.Index(typeName, "]")
		if bracketPos > 0 {
			keyPart := typeName[:bracketPos+1]   // e.g., "map[string]"
			valueType := typeName[bracketPos+1:] // e.g., "github.com/rail44/mantra/examples/simple.cacheItem"

			// Simplify the value type
			simplifiedValue := l.simplifyTypeName(valueType)
			return keyPart + simplifiedValue
		}
	}

	// Handle slice types
	if strings.HasPrefix(typeName, "[]") {
		elemType := typeName[2:]
		return "[]" + l.simplifyTypeName(elemType)
	}

	// Handle pointer types
	if strings.HasPrefix(typeName, "*") {
		baseType := typeName[1:]
		return "*" + l.simplifyTypeName(baseType)
	}

	// For regular types, use the general simplification
	return l.simplifyTypeName(typeName)
}

// simplifyTypeName simplifies type names based on import context
func (l *PackageLoader) simplifyTypeName(typeName string) string {
	if l.pkg == nil {
		return typeName
	}

	// For types with package qualification (contains dot)
	if strings.Contains(typeName, ".") {
		// Find the last dot to separate package from type name
		lastDot := strings.LastIndex(typeName, ".")
		if lastDot > 0 {
			pkgPath := typeName[:lastDot]
			typeNamePart := typeName[lastDot+1:]

			// Check if this is the current package
			if pkgPath == l.pkg.PkgPath {
				// Same package - no qualification needed
				return typeNamePart
			}

			// Check imports to find the correct alias
			if l.targetImports != nil {
				for _, imp := range l.targetImports {
					if imp.Path == pkgPath {
						// Skip blank imports - they can't be referenced directly
						if imp.IsBlank {
							// Return the full path as we can't use blank imports
							// The generated code will need to add a proper import
							return typeName
						}
						// Found the import - use its identifier
						identifier := imp.GetIdentifier()
						return identifier + "." + typeNamePart
					}
				}
			}

			// If not found in imports, try to extract package name from path
			// This handles cases where the full package path is in the type name
			if strings.Contains(pkgPath, "/") {
				// Extract the last segment as package name
				segments := strings.Split(pkgPath, "/")
				packageName := segments[len(segments)-1]

				// Check if this package is imported
				if l.targetImports != nil {
					for _, imp := range l.targetImports {
						if imp.Path == pkgPath || strings.HasSuffix(imp.Path, "/"+packageName) {
							// Skip blank imports
							if imp.IsBlank {
								return typeName
							}
							identifier := imp.GetIdentifier()
							return identifier + "." + typeNamePart
						}
					}
				}

				// Default to using the last segment
				return packageName + "." + typeNamePart
			}

			// For simple package names (like "time"), keep as is
			return typeName
		}
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
