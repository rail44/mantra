package context

import (
	"bytes"
	"fmt"
	"go/ast"
	"go/doc"
	"go/format"
	"go/types"
	"path/filepath"
	"strings"

	"github.com/rail44/mantra/internal/analysis"
	"golang.org/x/tools/go/packages"
)

// PackageLoader provides go/packages based type resolution
type PackageLoader struct {
	packagePath string
	pkg         *packages.Package
}

// NewPackageLoader creates a new package loader
func NewPackageLoader(packagePath string) *PackageLoader {
	return &PackageLoader{
		packagePath: packagePath,
	}
}

// Load loads the package if not already loaded
func (l *PackageLoader) Load() error {
	if l.pkg != nil {
		return nil // Already loaded
	}

	cfg := &packages.Config{
		Mode: packages.NeedTypes |
			packages.NeedSyntax |
			packages.NeedTypesInfo |
			packages.NeedName |
			packages.NeedFiles |
			packages.NeedImports |
			packages.NeedDeps,
		Dir:   l.packagePath,
		Tests: false,
	}

	pkgs, err := packages.Load(cfg, ".")
	if err != nil {
		return fmt.Errorf("failed to load package: %w", err)
	}
	if len(pkgs) == 0 {
		return fmt.Errorf("no packages found")
	}
	l.pkg = pkgs[0]
	return nil
}

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

// GetTypeInfo gets complete type information including methods
func (l *PackageLoader) GetTypeInfo(typeName string) (*TypeInfo, error) {
	if err := l.Load(); err != nil {
		return nil, err
	}

	obj := l.pkg.Types.Scope().Lookup(typeName)
	if obj == nil {
		return nil, fmt.Errorf("type %s not found", typeName)
	}

	info := &TypeInfo{
		Name:    typeName,
		Package: l.pkg.Name,
	}

	switch obj := obj.(type) {
	case *types.TypeName:
		l.extractTypeDetails(obj, info)
	default:
		return nil, fmt.Errorf("%s is not a type", typeName)
	}

	return info, nil
}

// extractTypeDetails extracts details from a type
func (l *PackageLoader) extractTypeDetails(obj *types.TypeName, info *TypeInfo) {
	typ := obj.Type()

	switch underlying := typ.Underlying().(type) {
	case *types.Struct:
		info.Kind = "struct"
		info.Definition = fmt.Sprintf("type %s struct", obj.Name())

		// Extract fields
		for i := 0; i < underlying.NumFields(); i++ {
			field := underlying.Field(i)
			info.Fields = append(info.Fields, analysis.FieldInfo{
				Name: field.Name(),
				Type: field.Type().String(),
			})
		}

		// Extract methods
		info.Methods = l.extractMethods(typ)

	case *types.Interface:
		info.Kind = "interface"
		info.Definition = fmt.Sprintf("type %s interface", obj.Name())

		// Extract interface methods
		for i := 0; i < underlying.NumMethods(); i++ {
			method := underlying.Method(i)
			sig := method.Type().(*types.Signature)
			info.Methods = append(info.Methods, analysis.MethodInfo{
				Name:      method.Name(),
				Signature: l.formatSignature(method.Name(), sig),
			})
		}

	default:
		info.Kind = "type"
		info.Type = underlying.String()
		info.Definition = fmt.Sprintf("type %s %s", obj.Name(), underlying)
	}
}

// getPackageDocs extracts documentation from a packages.Package
func (l *PackageLoader) getPackageDocs(pkg *packages.Package) (*doc.Package, error) {
	if pkg == nil || len(pkg.Syntax) == 0 {
		return nil, nil // No syntax available, can't extract docs
	}

	// Create doc.Package directly from AST files using the modern API
	docPkg, err := doc.NewFromFiles(pkg.Fset, pkg.Syntax, pkg.PkgPath)
	if err != nil {
		return nil, fmt.Errorf("failed to create doc package: %w", err)
	}

	return docPkg, nil
}

// findMethod finds a method by name in a type
func (l *PackageLoader) findMethod(typ types.Type, name string) *types.Func {
	// Check both pointer and value method sets
	ptrMethodSet := types.NewMethodSet(types.NewPointer(typ))

	for i := 0; i < ptrMethodSet.Len(); i++ {
		method := ptrMethodSet.At(i).Obj().(*types.Func)
		if method.Name() == name {
			return method
		}
	}

	return nil
}

// extractMethods gets all methods for a type
func (l *PackageLoader) extractMethods(typ types.Type) []analysis.MethodInfo {
	var methods []analysis.MethodInfo

	// Get method set for both value and pointer types
	mset := types.NewMethodSet(typ)
	ptrMset := types.NewMethodSet(types.NewPointer(typ))

	seen := make(map[string]bool)

	// Add methods from pointer type (includes all methods)
	for i := 0; i < ptrMset.Len(); i++ {
		method := ptrMset.At(i).Obj().(*types.Func)
		sig := method.Type().(*types.Signature)

		methodInfo := analysis.MethodInfo{
			Name:      method.Name(),
			Signature: l.formatSignature(method.Name(), sig),
			Receiver:  "*" + strings.TrimPrefix(typ.String(), "*"),
		}

		// Check if it's a value receiver method
		if i < mset.Len() {
			valueMethod := mset.At(i).Obj().(*types.Func)
			if valueMethod.Name() == method.Name() {
				methodInfo.Receiver = l.simplifyTypeName(strings.TrimPrefix(typ.String(), "*"))
			}
		}

		if !seen[method.Name()] {
			methods = append(methods, methodInfo)
			seen[method.Name()] = true
		}
	}

	return methods
}

// formatSignature formats a function/method signature
func (l *PackageLoader) formatSignature(name string, sig *types.Signature) string {
	// Parameters
	params := sig.Params()
	var paramStrs []string
	for i := 0; i < params.Len(); i++ {
		param := params.At(i)
		paramStr := param.Type().String()
		if param.Name() != "" {
			paramStr = param.Name() + " " + paramStr
		}
		paramStrs = append(paramStrs, paramStr)
	}

	// Results
	results := sig.Results()
	var resultStrs []string
	for i := 0; i < results.Len(); i++ {
		result := results.At(i)
		resultStrs = append(resultStrs, result.Type().String())
	}

	// Format signature
	signatureStr := name + "(" + strings.Join(paramStrs, ", ") + ")"

	if len(resultStrs) == 1 {
		signatureStr += " " + resultStrs[0]
	} else if len(resultStrs) > 1 {
		signatureStr += " (" + strings.Join(resultStrs, ", ") + ")"
	}

	return signatureStr
}

// GetAllTypes returns all types defined in the package
func (l *PackageLoader) GetAllTypes() (map[string]*TypeInfo, error) {
	if err := l.Load(); err != nil {
		return nil, err
	}

	typeInfos := make(map[string]*TypeInfo)

	// Iterate through all objects in package scope
	scope := l.pkg.Types.Scope()
	for _, name := range scope.Names() {
		obj := scope.Lookup(name)
		if _, ok := obj.(*types.TypeName); ok {
			info, err := l.GetTypeInfo(name)
			if err == nil {
				typeInfos[name] = info
			}
		}
	}

	return typeInfos, nil
}

// TypeInfo holds complete information about a type
type TypeInfo struct {
	Name       string
	Kind       string // "struct", "interface", "type"
	Package    string
	Definition string
	Type       string // For type aliases
	Fields     []analysis.FieldInfo
	Methods    []analysis.MethodInfo
}

// extractMethodsForDeclaration gets methods for use in Declaration types
func (l *PackageLoader) extractMethodsForDeclaration(typ types.Type) []MethodInfo {
	var methods []MethodInfo

	// Get method set for both value and pointer types
	mset := types.NewMethodSet(typ)
	ptrMset := types.NewMethodSet(types.NewPointer(typ))

	seen := make(map[string]bool)

	// Add methods from pointer type (includes all methods)
	for i := 0; i < ptrMset.Len(); i++ {
		method := ptrMset.At(i).Obj().(*types.Func)
		sig := method.Type().(*types.Signature)

		methodInfo := MethodInfo{
			Name:      method.Name(),
			Signature: l.formatSignature(method.Name(), sig),
			Receiver:  l.simplifyTypeName("*" + strings.TrimPrefix(typ.String(), "*")),
		}

		// Check if it's a value receiver method
		if i < mset.Len() {
			valueMethod := mset.At(i).Obj().(*types.Func)
			if valueMethod.Name() == method.Name() {
				methodInfo.Receiver = l.simplifyTypeName(strings.TrimPrefix(typ.String(), "*"))
			}
		}

		if !seen[method.Name()] {
			methods = append(methods, methodInfo)
			seen[method.Name()] = true
		}
	}

	return methods
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

		// Handle map types
		// e.g., "map[string]github.com/rail44/mantra/examples/simple.cacheItem" -> "map[string]cacheItem"
		typeName = strings.ReplaceAll(typeName, prefix, "")
	}

	return typeName
}

// getFunctionImplementation extracts function body from AST
func (l *PackageLoader) getFunctionImplementation(funcName string) string {
	// Find the function in AST
	for _, file := range l.pkg.Syntax {
		for _, decl := range file.Decls {
			if funcDecl, ok := decl.(*ast.FuncDecl); ok {
				if funcDecl.Name.Name == funcName && funcDecl.Body != nil {
					// Format the function body
					var buf bytes.Buffer
					if err := format.Node(&buf, l.pkg.Fset, funcDecl.Body); err == nil {
						// Remove the outer braces
						body := buf.String()
						if len(body) > 2 && body[0] == '{' && body[len(body)-1] == '}' {
							body = strings.TrimSpace(body[1 : len(body)-1])
						}
						return body
					}
				}
			}
		}
	}
	return ""
}

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
	if typeInfo.Kind != "struct" {
		// For non-struct types, use the original definition
		return typeInfo.Definition
	}

	// For struct types, build complete definition with fields
	var builder strings.Builder
	builder.WriteString(fmt.Sprintf("type %s struct", typeInfo.Name))

	if len(typeInfo.Fields) == 0 {
		builder.WriteString(" {}")
		return builder.String()
	}

	builder.WriteString(" {\n")
	for _, field := range typeInfo.Fields {
		// Simplify type names for same-package types
		fieldType := l.simplifyFieldTypeName(field.Type)
		builder.WriteString(fmt.Sprintf("\t%s %s", field.Name, fieldType))
		if field.Tag != "" {
			builder.WriteString(fmt.Sprintf(" `%s`", field.Tag))
		}
		builder.WriteString("\n")
	}
	builder.WriteString("}")

	return builder.String()
}

// simplifyFieldTypeName simplifies field type names using import and package scope information
func (l *PackageLoader) simplifyFieldTypeName(typeName string) string {
	if l.pkg == nil {
		return typeName
	}

	// Handle same-package types first
	pkgPath := l.pkg.PkgPath
	if pkgPath != "" && strings.Contains(typeName, pkgPath+".") {
		// Replace "map[string]github.com/user/repo/package.Type" with "map[string]Type"
		simplifiedType := strings.ReplaceAll(typeName, pkgPath+".", "")
		return simplifiedType
	}

	// Handle imported package types
	for importPath, importedPkg := range l.pkg.Imports {
		if importedPkg != nil && strings.Contains(typeName, importPath+".") {
			// Get the package identifier (name or alias)
			pkgName := importedPkg.Name
			if pkgName == "" {
				pkgName = filepath.Base(importPath)
			}

			// Replace full import path with package identifier
			// e.g., "time.Time" instead of "time.github.com/golang/time.Time"
			simplifiedType := strings.ReplaceAll(typeName, importPath+".", pkgName+".")
			return simplifiedType
		}
	}

	return typeName
}

// resolveQualifiedName resolves a qualified name by first checking imports
func (l *PackageLoader) resolveQualifiedName(parts []string) (Declaration, error) {
	if len(parts) == 0 {
		return nil, fmt.Errorf("empty name")
	}

	first := parts[0]

	// First, check if the first part matches an imported package
	for importPath, importedPkg := range l.pkg.Imports {
		pkgName := importedPkg.Name
		if pkgName == "" {
			pkgName = filepath.Base(importPath)
		}

		if pkgName == first {
			// Found in imports - resolve in external package
			remaining := parts[1:]
			if len(remaining) == 0 {
				// Just the package name
				return &baseDeclaration{
					Found:   true,
					Name:    pkgName,
					Kind:    "package",
					Package: importPath,
				}, nil
			}

			// Check if the imported package has type information
			if importedPkg.Types == nil || importedPkg.Types.Scope() == nil {
				// Need to load the imported package explicitly, including syntax for documentation
				cfg := &packages.Config{
					Mode: packages.NeedTypes | packages.NeedName | packages.NeedSyntax | packages.NeedTypesInfo,
				}
				pkgs, err := packages.Load(cfg, importPath)
				if err == nil && len(pkgs) > 0 {
					importedPkg = pkgs[0]
				}
			}

			return l.resolveInPackage(importedPkg, remaining, pkgName)
		}
	}

	// Not found in imports - resolve in current package
	return l.resolveInPackage(l.pkg, parts, l.pkg.Name)
}

// resolveInPackage resolves a name within a specific package
func (l *PackageLoader) resolveInPackage(pkg *packages.Package, parts []string, pkgName string) (Declaration, error) {
	if len(parts) == 0 || pkg == nil || pkg.Types == nil {
		return nil, fmt.Errorf("invalid package or empty parts")
	}

	// Simple lookup in package scope
	name := parts[0]
	obj := pkg.Types.Scope().Lookup(name)
	if obj == nil {
		return nil, fmt.Errorf("%s not found in package %s", name, pkgName)
	}

	// If there are more parts, handle nested access like Type.Method
	if len(parts) > 1 {
		return l.resolveNestedAccess(obj, parts[1:], pkgName)
	}

	// Create declaration with appropriate package info
	decl, err := l.createDeclarationFromObjectWithPackage(obj, pkgName)
	if err != nil {
		return nil, err
	}

	// Try to extract documentation if possible
	if funcDecl, ok := decl.(*FunctionDeclaration); ok && len(pkg.Syntax) > 0 {
		if docPkg, err := l.getPackageDocs(pkg); err == nil && docPkg != nil {
			// Look for the function in doc.Package
			for _, f := range docPkg.Funcs {
				if f.Name == name {
					funcDecl.Doc = f.Doc
					break
				}
			}
		}
	}

	return decl, nil
}

// resolveNestedAccess resolves nested access like Type.Method
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
	default:
		return nil, fmt.Errorf("cannot access members of %T", obj)
	}

	// For the remaining parts, look for methods or fields
	memberName := parts[0]

	// Try to find as a method
	if method := l.findMethod(typ, memberName); method != nil {
		if len(parts) > 1 {
			// Can't have more nested access after a method
			return nil, fmt.Errorf("cannot access members of method %s", memberName)
		}
		return l.getFunctionDeclarationWithPackage(method, pkgName)
	}

	// Try to find as a field (for structs)
	if structType, ok := typ.Underlying().(*types.Struct); ok {
		for i := 0; i < structType.NumFields(); i++ {
			field := structType.Field(i)
			if field.Name() == memberName {
				// Continue resolving with the field's type
				if len(parts) > 1 {
					return l.resolveNestedAccess(field, parts[1:], pkgName)
				}
				// Return field information
				return &baseDeclaration{
					Found:   true,
					Name:    field.Name(),
					Kind:    "field",
					Package: pkgName,
				}, nil
			}
		}
	}

	return nil, fmt.Errorf("member %s not found in type %s", memberName, typ)
}

// createDeclarationFromObjectWithPackage creates a Declaration with specific package info
func (l *PackageLoader) createDeclarationFromObjectWithPackage(obj types.Object, pkgName string) (Declaration, error) {
	switch obj := obj.(type) {
	case *types.TypeName:
		return l.getTypeDeclarationWithPackage(obj, pkgName)
	case *types.Func:
		return l.getFunctionDeclarationWithPackage(obj, pkgName)
	case *types.Const:
		return l.getConstantDeclarationWithPackage(obj, pkgName)
	case *types.Var:
		return l.getVariableDeclarationWithPackage(obj, pkgName)
	default:
		return nil, fmt.Errorf("unknown declaration kind")
	}
}

// getTypeDeclarationWithPackage converts a TypeName to appropriate Declaration with specific package name
func (l *PackageLoader) getTypeDeclarationWithPackage(obj *types.TypeName, pkgName string) (Declaration, error) {
	typ := obj.Type()

	switch underlying := typ.Underlying().(type) {
	case *types.Struct:
		result := &StructDeclaration{
			baseDeclaration: baseDeclaration{
				Found:   true,
				Name:    obj.Name(),
				Kind:    "struct",
				Package: pkgName,
			},
			Definition: fmt.Sprintf("type %s struct", obj.Name()),
		}

		// Extract fields
		for i := 0; i < underlying.NumFields(); i++ {
			field := underlying.Field(i)
			result.Fields = append(result.Fields, FieldInfo{
				Name: field.Name(),
				Type: l.simplifyTypeName(field.Type().String()),
			})
		}

		// Extract methods
		result.Methods = l.extractMethodsForDeclaration(typ)
		return result, nil

	case *types.Interface:
		result := &InterfaceDeclaration{
			baseDeclaration: baseDeclaration{
				Found:   true,
				Name:    obj.Name(),
				Kind:    "interface",
				Package: pkgName,
			},
			Definition: fmt.Sprintf("type %s interface", obj.Name()),
		}

		// Extract interface methods
		for i := 0; i < underlying.NumMethods(); i++ {
			method := underlying.Method(i)
			sig := method.Type().(*types.Signature)
			result.Methods = append(result.Methods, MethodInfo{
				Name:      method.Name(),
				Signature: l.formatSignature(method.Name(), sig),
			})
		}
		return result, nil

	default:
		// Type alias or other type
		result := &TypeAliasDeclaration{
			baseDeclaration: baseDeclaration{
				Found:   true,
				Name:    obj.Name(),
				Kind:    "type",
				Package: pkgName,
			},
			Definition: fmt.Sprintf("type %s %s", obj.Name(), underlying),
			Type:       underlying.String(),
		}

		// Extract methods for the type (e.g., time.Duration has methods)
		result.Methods = l.extractMethodsForDeclaration(typ)

		return result, nil
	}
}

// getFunctionDeclarationWithPackage converts a Func to FunctionDeclaration with specific package name
func (l *PackageLoader) getFunctionDeclarationWithPackage(obj *types.Func, pkgName string) (Declaration, error) {
	sig := obj.Type().(*types.Signature)

	kind := "func"
	if sig.Recv() != nil {
		kind = "method"
	}

	result := &FunctionDeclaration{
		baseDeclaration: baseDeclaration{
			Found:   true,
			Name:    obj.Name(),
			Kind:    kind,
			Package: pkgName,
		},
		Signature: l.formatSignature(obj.Name(), sig),
	}

	// Check if it's a method
	if sig.Recv() != nil {
		recv := sig.Recv()
		if recv.Type() != nil {
			result.Receiver = recv.Type().String()
		}
	}

	// Get implementation if available (only for current package)
	if pkgName == l.pkg.Name {
		implementation := l.getFunctionImplementation(obj.Name())
		if implementation != "" {
			result.Implementation = implementation
		}
	}

	return result, nil
}

// getConstantDeclarationWithPackage converts a Const to ConstantDeclaration with specific package name
func (l *PackageLoader) getConstantDeclarationWithPackage(obj *types.Const, pkgName string) (Declaration, error) {
	result := &ConstantDeclaration{
		baseDeclaration: baseDeclaration{
			Found:   true,
			Name:    obj.Name(),
			Kind:    "const",
			Package: pkgName,
		},
		Type: obj.Type().String(),
	}

	if obj.Val() != nil {
		result.Value = obj.Val().String()
	}

	return result, nil
}

// getVariableDeclarationWithPackage converts a Var to VariableDeclaration with specific package name
func (l *PackageLoader) getVariableDeclarationWithPackage(obj *types.Var, pkgName string) (Declaration, error) {
	result := &VariableDeclaration{
		baseDeclaration: baseDeclaration{
			Found:   true,
			Name:    obj.Name(),
			Kind:    "var",
			Package: pkgName,
		},
		Type: obj.Type().String(),
	}

	// TODO: Extract init pattern if needed

	return result, nil
}
