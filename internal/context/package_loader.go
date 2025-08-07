package context

import (
	"bytes"
	"fmt"
	"go/ast"
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
			packages.NeedFiles,
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
func (l *PackageLoader) GetDeclaration(name string) (Declaration, error) {
	if err := l.Load(); err != nil {
		return nil, err
	}

	obj := l.pkg.Types.Scope().Lookup(name)
	if obj == nil {
		return nil, fmt.Errorf("declaration %s not found", name)
	}

	switch obj := obj.(type) {
	case *types.TypeName:
		return l.getTypeDeclaration(obj)
	case *types.Func:
		return l.getFunctionDeclaration(obj)
	case *types.Const:
		return l.getConstantDeclaration(obj)
	case *types.Var:
		return l.getVariableDeclaration(obj)
	default:
		return nil, fmt.Errorf("unknown declaration kind for %s", name)
	}
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

// getTypeDeclaration converts a TypeName to appropriate Declaration
func (l *PackageLoader) getTypeDeclaration(obj *types.TypeName) (Declaration, error) {
	typ := obj.Type()

	switch underlying := typ.Underlying().(type) {
	case *types.Struct:
		result := &StructDeclaration{
			baseDeclaration: baseDeclaration{
				Found:   true,
				Name:    obj.Name(),
				Kind:    "struct",
				Package: l.pkg.Name,
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
				Package: l.pkg.Name,
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
				Package: l.pkg.Name,
			},
			Definition: fmt.Sprintf("type %s %s", obj.Name(), underlying),
			Type:       underlying.String(),
		}
		return result, nil
	}
}

// getFunctionDeclaration converts a Func to FunctionDeclaration
func (l *PackageLoader) getFunctionDeclaration(obj *types.Func) (Declaration, error) {
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
			Package: l.pkg.Name,
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

	// Get implementation if available
	implementation := l.getFunctionImplementation(obj.Name())
	if implementation != "" {
		result.Implementation = implementation
	}

	return result, nil
}

// getConstantDeclaration converts a Const to ConstantDeclaration
func (l *PackageLoader) getConstantDeclaration(obj *types.Const) (Declaration, error) {
	result := &ConstantDeclaration{
		baseDeclaration: baseDeclaration{
			Found:   true,
			Name:    obj.Name(),
			Kind:    "const",
			Package: l.pkg.Name,
		},
		Type: obj.Type().String(),
	}

	if obj.Val() != nil {
		result.Value = obj.Val().String()
	}

	return result, nil
}

// getVariableDeclaration converts a Var to VariableDeclaration
func (l *PackageLoader) getVariableDeclaration(obj *types.Var) (Declaration, error) {
	result := &VariableDeclaration{
		baseDeclaration: baseDeclaration{
			Found:   true,
			Name:    obj.Name(),
			Kind:    "var",
			Package: l.pkg.Name,
		},
		Type: obj.Type().String(),
	}

	// TODO: Extract init pattern if needed

	return result, nil
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
			ctx.Types[typeName] = typeInfo.Definition
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
						ctx.Types[refType] = typeInfo.Definition
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
