package context

import (
	"fmt"
	"go/types"
	"strings"

	"golang.org/x/tools/go/packages"
)

// GetDeclaration returns information about any declaration
// Supports both local declarations and qualified names (e.g., "time.Time")
func (l *PackageLoader) GetDeclaration(name string) (Declaration, error) {
	if err := l.Load(); err != nil {
		return nil, err
	}

	// Split by dots and resolve step by step
	parts := strings.Split(name, ".")
	return l.resolveQualifiedName(parts)
}

// resolveQualifiedName resolves a qualified name step by step
func (l *PackageLoader) resolveQualifiedName(parts []string) (Declaration, error) {
	if len(parts) == 0 {
		return &NotFoundDeclaration{
			baseDeclaration: baseDeclaration{
				Name:  strings.Join(parts, "."),
				Kind:  "not_found",
				Found: false,
			},
		}, nil
	}

	// First, check if it's in the current package scope
	obj := l.pkg.Types.Scope().Lookup(parts[0])
	if obj != nil {
		if len(parts) == 1 {
			// Direct reference to a declaration in this package
			return l.createDeclarationFromObjectWithPackageAndPkg(obj, l.pkg.Name, l.pkg)
		}
		// Nested access (e.g., TypeName.FieldName)
		return l.resolveNestedAccess(obj, parts[1:], l.pkg.Name)
	}

	// Check if it's an imported package
	for _, imp := range l.pkg.Imports {
		pkgName := imp.Name
		if pkgName == imp.PkgPath {
			// Extract package name from path
			if idx := strings.LastIndex(pkgName, "/"); idx >= 0 {
				pkgName = pkgName[idx+1:]
			}
		}

		if pkgName == parts[0] {
			if len(parts) == 1 {
				// Reference to the package itself
				return &NotFoundDeclaration{
					baseDeclaration: baseDeclaration{
						Name:  parts[0],
						Kind:  "not_found",
						Found: false,
					},
				}, nil
			}
			// Try to resolve in imported package
			return l.resolveInPackage(imp, parts[1:], imp.Name)
		}
	}

	// Not found
	return &NotFoundDeclaration{baseDeclaration: baseDeclaration{Name: strings.Join(parts, "."), Kind: "not_found", Found: false}}, nil
}

// resolveInPackage resolves a name within a specific package
func (l *PackageLoader) resolveInPackage(pkg *packages.Package, parts []string, pkgName string) (Declaration, error) {
	if len(parts) == 0 || pkg.Types == nil || pkg.Types.Scope() == nil {
		return &NotFoundDeclaration{
			baseDeclaration: baseDeclaration{
				Name:  strings.Join(parts, "."),
				Kind:  "not_found",
				Found: false,
			},
		}, nil
	}

	obj := pkg.Types.Scope().Lookup(parts[0])
	if obj == nil {
		return &NotFoundDeclaration{
			baseDeclaration: baseDeclaration{
				Name:  strings.Join(parts, "."),
				Kind:  "not_found",
				Found: false,
			},
		}, nil
	}

	if len(parts) == 1 {
		return l.createDeclarationFromObjectWithPackageAndPkg(obj, pkgName, pkg)
	}

	// Nested access within the imported package
	return l.resolveNestedAccess(obj, parts[1:], pkgName)
}

// resolveNestedAccess resolves nested field/method access
func (l *PackageLoader) resolveNestedAccess(obj types.Object, parts []string, pkgName string) (Declaration, error) {
	if len(parts) == 0 {
		return l.createDeclarationFromObjectWithPackage(obj, pkgName)
	}

	// Get the type of the object
	var typ types.Type
	switch o := obj.(type) {
	case *types.TypeName:
		typ = o.Type()
	case *types.Var:
		typ = o.Type()
	case *types.Const:
		typ = o.Type()
	default:
		return &NotFoundDeclaration{baseDeclaration: baseDeclaration{Name: obj.Name() + "." + strings.Join(parts, "."), Kind: "not_found", Found: false}}, nil
	}

	// For each remaining part, try to find it as a field or method
	for i, part := range parts {
		// Dereference pointer types
		if ptr, ok := typ.(*types.Pointer); ok {
			typ = ptr.Elem()
		}

		// Look for the field or method
		switch t := typ.Underlying().(type) {
		case *types.Struct:
			// Look for field
			for j := 0; j < t.NumFields(); j++ {
				field := t.Field(j)
				if field.Name() == part {
					if i == len(parts)-1 {
						// This is the final part
						return &VariableDeclaration{
							baseDeclaration: baseDeclaration{
								Name:    part,
								Kind:    "field",
								Package: pkgName,
								Found:   true,
							},
							Type:        field.Type().String(),
							InitPattern: "", // Fields don't have init patterns
						}, nil
					}
					// Continue with the field's type
					typ = field.Type()
					break
				}
			}
		default:
			// Not a struct, can't have fields
			return &NotFoundDeclaration{baseDeclaration: baseDeclaration{Name: obj.Name() + "." + strings.Join(parts, "."), Kind: "not_found", Found: false}}, nil
		}
	}

	return &NotFoundDeclaration{baseDeclaration: baseDeclaration{Name: obj.Name() + "." + strings.Join(parts, "."), Kind: "not_found", Found: false}}, nil
}

// createDeclarationFromObjectWithPackage creates a Declaration from a types.Object
func (l *PackageLoader) createDeclarationFromObjectWithPackage(obj types.Object, pkgName string) (Declaration, error) {
	return l.createDeclarationFromObjectWithPackageAndPkg(obj, pkgName, nil)
}

// createDeclarationFromObjectWithPackageAndPkg creates a Declaration with package context
func (l *PackageLoader) createDeclarationFromObjectWithPackageAndPkg(obj types.Object, pkgName string, pkg *packages.Package) (Declaration, error) {
	switch o := obj.(type) {
	case *types.TypeName:
		return l.getTypeDeclarationWithPackageAndPkg(o, pkgName, pkg)
	case *types.Func:
		return l.getFunctionDeclarationWithPackage(o, pkgName)
	case *types.Const:
		return l.getConstantDeclarationWithPackage(o, pkgName)
	case *types.Var:
		return l.getVariableDeclarationWithPackage(o, pkgName)
	default:
		return &NotFoundDeclaration{baseDeclaration: baseDeclaration{Name: obj.Name(), Kind: "not_found", Found: false}}, nil
	}
}

// getTypeDeclarationWithPackageAndPkg gets type declaration with documentation
func (l *PackageLoader) getTypeDeclarationWithPackageAndPkg(obj *types.TypeName, pkgName string, pkg *packages.Package) (Declaration, error) {
	typ := obj.Type()

	switch t := typ.Underlying().(type) {
	case *types.Struct:
		result := &StructDeclaration{
			baseDeclaration: baseDeclaration{
				Name:    obj.Name(),
				Kind:    "struct",
				Package: pkgName,
				Found:   true,
			},
			Fields: []FieldInfo{},
		}

		// Extract fields
		for i := 0; i < t.NumFields(); i++ {
			field := t.Field(i)
			result.Fields = append(result.Fields, FieldInfo{
				Name: field.Name(),
				Type: l.simplifyFieldTypeName(field.Type().String()),
				Tag:  t.Tag(i),
			})
		}

		// Extract methods
		result.Methods = l.extractMethodsForDeclarationWithDoc(typ, pkg, obj.Name())

		// Format definition
		result.Definition = l.formatStructDefinition(obj.Name(), result.Fields)

		// Attach documentation if available
		if pkg != nil {
			l.attachDocumentation(result, obj.Name(), pkg)
		}

		return result, nil

	case *types.Interface:
		result := &InterfaceDeclaration{
			baseDeclaration: baseDeclaration{
				Name:    obj.Name(),
				Kind:    "interface",
				Package: pkgName,
				Found:   true,
			},
			Methods: []MethodInfo{},
		}

		// Extract interface methods
		for i := 0; i < t.NumMethods(); i++ {
			method := t.Method(i)
			sig := method.Type().(*types.Signature)
			result.Methods = append(result.Methods, MethodInfo{
				Name:      method.Name(),
				Signature: l.formatSignature(method.Name(), sig),
			})
		}

		// Extract additional methods for named types
		result.Methods = append(result.Methods, l.extractMethodsForDeclarationWithDoc(typ, pkg, obj.Name())...)

		// Format definition
		result.Definition = l.formatInterfaceDefinition(obj.Name(), result.Methods)

		// Attach documentation if available
		if pkg != nil {
			l.attachDocumentation(result, obj.Name(), pkg)
		}

		return result, nil

	default:
		// Type alias or basic type definition
		result := &TypeAliasDeclaration{
			baseDeclaration: baseDeclaration{
				Name:    obj.Name(),
				Kind:    "type",
				Package: pkgName,
				Found:   true,
			},
			Type:       t.String(),
			Definition: fmt.Sprintf("type %s %s", obj.Name(), t),
		}

		// Attach documentation if available
		if pkg != nil {
			l.attachDocumentation(result, obj.Name(), pkg)
		}

		return result, nil
	}
}

// getFunctionDeclarationWithPackage creates a function declaration
func (l *PackageLoader) getFunctionDeclarationWithPackage(obj *types.Func, pkgName string) (Declaration, error) {
	sig := obj.Type().(*types.Signature)

	result := &FunctionDeclaration{
		baseDeclaration: baseDeclaration{
			Name:    obj.Name(),
			Kind:    "function",
			Package: pkgName,
			Found:   true,
		},
		Signature: l.formatSignature(obj.Name(), sig),
	}

	// Check if it's a method
	if recv := sig.Recv(); recv != nil {
		result.Kind = "method"
		result.Receiver = recv.Type().String()
	}

	// Try to get implementation
	implementation := l.getFunctionImplementation(obj.Name())
	if implementation != "" {
		result.Implementation = implementation
	}

	return result, nil
}

// getConstantDeclarationWithPackage creates a constant declaration
func (l *PackageLoader) getConstantDeclarationWithPackage(obj *types.Const, pkgName string) (Declaration, error) {
	result := &ConstantDeclaration{
		baseDeclaration: baseDeclaration{
			Name:    obj.Name(),
			Kind:    "constant",
			Package: pkgName,
			Found:   true,
		},
		Type:  obj.Type().String(),
		Value: obj.Val().String(),
	}

	return result, nil
}

// getVariableDeclarationWithPackage creates a variable declaration
func (l *PackageLoader) getVariableDeclarationWithPackage(obj *types.Var, pkgName string) (Declaration, error) {
	result := &VariableDeclaration{
		baseDeclaration: baseDeclaration{
			Name:    obj.Name(),
			Kind:    "variable",
			Package: pkgName,
			Found:   true,
		},
		Type: obj.Type().String(),
	}

	return result, nil
}

// formatStructDefinition formats a struct definition
func (l *PackageLoader) formatStructDefinition(name string, fields []FieldInfo) string {
	var builder strings.Builder
	builder.WriteString(fmt.Sprintf("type %s struct {\n", name))
	for _, field := range fields {
		builder.WriteString(fmt.Sprintf("    %s %s", field.Name, field.Type))
		if field.Tag != "" {
			builder.WriteString(fmt.Sprintf(" `%s`", field.Tag))
		}
		builder.WriteString("\n")
	}
	builder.WriteString("}")
	return builder.String()
}

// formatInterfaceDefinition formats an interface definition
func (l *PackageLoader) formatInterfaceDefinition(name string, methods []MethodInfo) string {
	var builder strings.Builder
	builder.WriteString(fmt.Sprintf("type %s interface {\n", name))
	for _, method := range methods {
		builder.WriteString(fmt.Sprintf("    %s\n", method.Signature))
	}
	builder.WriteString("}")
	return builder.String()
}
