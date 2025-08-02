package context

import (
	"go/ast"
	"go/importer"
	goparser "go/parser"
	"go/token"
	"go/types"
	"strings"

	astutils "github.com/rail44/mantra/internal/ast"
	"github.com/rail44/mantra/internal/parser"
)

// RelevantContext holds context information relevant to a function
type RelevantContext struct {
	Imports      []string          // Import statements
	Types        map[string]string // Type definitions (name -> definition)
	Constants    map[string]string // Constant definitions
	Variables    map[string]string // Variable definitions
	Functions    map[string]string // Other function signatures that might be called
	PackageName  string            // Package name
	ReceiverType string            // Full type definition if the target is a method
}

// ExtractRelevantContext analyzes the file and extracts context relevant to the target function
func ExtractRelevantContext(fileContent string, target *parser.Target) (*RelevantContext, error) {
	fset := token.NewFileSet()
	node, err := goparser.ParseFile(fset, target.FilePath, fileContent, goparser.ParseComments)
	if err != nil {
		return nil, err
	}

	ctx := &RelevantContext{
		Types:        make(map[string]string),
		Constants:    make(map[string]string),
		Variables:    make(map[string]string),
		Functions:    make(map[string]string),
		PackageName:  node.Name.Name,
		ReceiverType: "",
	}

	// Type check the package for complete type information
	conf := types.Config{
		Importer: importer.Default(),
		Error:    func(err error) {}, // Ignore type errors for now
	}

	info := &types.Info{
		Types: make(map[ast.Expr]types.TypeAndValue),
		Defs:  make(map[*ast.Ident]types.Object),
		Uses:  make(map[*ast.Ident]types.Object),
	}

	pkg, _ := conf.Check(ctx.PackageName, fset, []*ast.File{node}, info)

	// Extract imports - we'll filter these later based on usage
	allImports := make(map[string]string) // path -> import statement
	for _, imp := range node.Imports {
		path := strings.Trim(imp.Path.Value, `"`)
		if imp.Name != nil {
			allImports[path] = imp.Name.Name + " " + imp.Path.Value
		} else {
			allImports[path] = imp.Path.Value
		}
	}

	// Collect all type names used in the target function
	usedTypes := collectUsedTypes(target)

	// Extract relevant declarations
	for _, decl := range node.Decls {
		switch d := decl.(type) {
		case *ast.GenDecl:
			extractGenDecl(d, ctx, usedTypes, fileContent, fset)
		case *ast.FuncDecl:
			// Extract other functions that might be useful
			if d.Name.Name != target.Name {
				sig := extractFunctionSignature(d)
				ctx.Functions[d.Name.Name] = sig
			}
		}
	}

	// If target is a method, ensure we have the receiver type
	if target.Receiver != nil && target.Receiver.Type != "" {
		receiverType := strings.TrimPrefix(target.Receiver.Type, "*")
		if _, exists := ctx.Types[receiverType]; !exists {
			// Try to find the type definition
			ast.Inspect(node, func(n ast.Node) bool {
				if typeSpec, ok := n.(*ast.TypeSpec); ok && typeSpec.Name.Name == receiverType {
					ctx.Types[receiverType] = extractTypeDefinition(typeSpec, fileContent, fset)
					return false
				}
				return true
			})
		}
	}

	// Extract interface definitions using type information
	if pkg != nil {
		extractInterfacesFromTypes(ctx, pkg, info, node, fileContent, fset)
	}

	// Filter imports based on actual usage
	ctx.Imports = filterImportsByUsage(allImports, target, ctx, info)

	return ctx, nil
}

// collectUsedTypes collects all type names referenced in the target function
func collectUsedTypes(target *parser.Target) map[string]bool {
	types := make(map[string]bool)

	// Add receiver type
	if target.Receiver != nil {
		typeName := strings.TrimPrefix(target.Receiver.Type, "*")
		types[typeName] = true
	}

	// Add parameter types
	for _, param := range target.Params {
		extractTypeNames(param.Type, types)
	}

	// Add return types
	for _, ret := range target.Returns {
		extractTypeNames(ret.Type, types)
	}

	return types
}

// extractTypeNames extracts type names from a type string
func extractTypeNames(typeStr string, types map[string]bool) {
	// Remove slices, pointers, maps, channels
	typeStr = strings.TrimPrefix(typeStr, "[]")
	typeStr = strings.TrimPrefix(typeStr, "*")
	typeStr = strings.TrimPrefix(typeStr, "chan ")

	// Handle maps
	if strings.HasPrefix(typeStr, "map[") {
		// Extract both key and value types
		parts := strings.SplitN(typeStr[4:], "]", 2)
		if len(parts) == 2 {
			extractTypeNames(parts[0], types)
			extractTypeNames(parts[1], types)
		}
		return
	}

	// Handle qualified types (package.Type)
	if strings.Contains(typeStr, ".") {
		return // Skip imported types
	}

	// Add the type if it's not a built-in
	if !isBuiltinType(typeStr) && typeStr != "" {
		types[typeStr] = true
	}
}

// isBuiltinType checks if a type is a Go built-in type
func isBuiltinType(typeName string) bool {
	builtins := map[string]bool{
		"bool": true, "string": true, "error": true,
		"int": true, "int8": true, "int16": true, "int32": true, "int64": true,
		"uint": true, "uint8": true, "uint16": true, "uint32": true, "uint64": true,
		"uintptr": true, "byte": true, "rune": true,
		"float32": true, "float64": true,
		"complex64": true, "complex128": true,
		"interface{}": true, "any": true,
	}
	return builtins[typeName]
}

// extractGenDecl extracts relevant information from general declarations
func extractGenDecl(decl *ast.GenDecl, ctx *RelevantContext, usedTypes map[string]bool, fileContent string, fset *token.FileSet) {
	for _, spec := range decl.Specs {
		switch s := spec.(type) {
		case *ast.TypeSpec:
			if usedTypes[s.Name.Name] {
				ctx.Types[s.Name.Name] = extractTypeDefinition(s, fileContent, fset)
			}
		case *ast.ValueSpec:
			// Extract constants and variables
			for _, name := range s.Names {
				if decl.Tok == token.CONST {
					ctx.Constants[name.Name] = extractValueDefinition(s, fileContent, fset)
				} else if decl.Tok == token.VAR {
					ctx.Variables[name.Name] = extractValueDefinition(s, fileContent, fset)
				}
			}
		}
	}
}

// extractTypeDefinition extracts the full type definition
func extractTypeDefinition(spec *ast.TypeSpec, fileContent string, fset *token.FileSet) string {
	start := fset.Position(spec.Pos()).Offset
	end := fset.Position(spec.End()).Offset
	if start >= 0 && end <= len(fileContent) {
		return fileContent[start:end]
	}
	return ""
}

// extractValueDefinition extracts constant or variable definition
func extractValueDefinition(spec *ast.ValueSpec, fileContent string, fset *token.FileSet) string {
	start := fset.Position(spec.Pos()).Offset
	end := fset.Position(spec.End()).Offset
	if start >= 0 && end <= len(fileContent) {
		return fileContent[start:end]
	}
	return ""
}

// extractFunctionSignature extracts a function signature as a string
func extractFunctionSignature(fn *ast.FuncDecl) string {
	var sig strings.Builder
	sig.WriteString("func ")

	// Add receiver if present
	if fn.Recv != nil && len(fn.Recv.List) > 0 {
		sig.WriteString("(")
		recv := fn.Recv.List[0]
		if len(recv.Names) > 0 {
			sig.WriteString(recv.Names[0].Name)
			sig.WriteString(" ")
		}
		sig.WriteString(astutils.GetTypeString(recv.Type))
		sig.WriteString(") ")
	}

	// Add function name
	sig.WriteString(fn.Name.Name)
	sig.WriteString("(")

	// Add parameters
	if fn.Type.Params != nil {
		for i, field := range fn.Type.Params.List {
			if i > 0 {
				sig.WriteString(", ")
			}
			if len(field.Names) > 0 {
				for j, name := range field.Names {
					if j > 0 {
						sig.WriteString(", ")
					}
					sig.WriteString(name.Name)
				}
				sig.WriteString(" ")
			}
			sig.WriteString(astutils.GetTypeString(field.Type))
		}
	}
	sig.WriteString(")")

	// Add return types
	if fn.Type.Results != nil && len(fn.Type.Results.List) > 0 {
		sig.WriteString(" ")
		if len(fn.Type.Results.List) > 1 {
			sig.WriteString("(")
		}
		for i, field := range fn.Type.Results.List {
			if i > 0 {
				sig.WriteString(", ")
			}
			sig.WriteString(astutils.GetTypeString(field.Type))
		}
		if len(fn.Type.Results.List) > 1 {
			sig.WriteString(")")
		}
	}

	return sig.String()
}

// extractInterfacesFromTypes extracts interface definitions using type information
func extractInterfacesFromTypes(ctx *RelevantContext, pkg *types.Package, info *types.Info, node *ast.File, fileContent string, fset *token.FileSet) {
	// First, collect all types that are referenced in the context
	referencedTypes := make(map[string]bool)

	// Check all type definitions we've already collected
	for typeName := range ctx.Types {
		// Parse the type definition to find field types
		collectReferencedTypesFromTypedef(ctx.Types[typeName], referencedTypes)
	}

	// Always include types referenced directly in fields
	for name := range referencedTypes {
		if _, exists := ctx.Types[name]; !exists {
			// Type is referenced but not yet included
			usedTypes := map[string]bool{name: true}
			// Try to find it in the AST
			for _, decl := range node.Decls {
				if genDecl, ok := decl.(*ast.GenDecl); ok {
					extractGenDecl(genDecl, ctx, usedTypes, fileContent, fset)
				}
			}
		}
	}

	// Now look for interface definitions for these referenced types
	scope := pkg.Scope()
	for _, name := range scope.Names() {
		if !referencedTypes[name] {
			continue
		}

		obj := scope.Lookup(name)
		if obj == nil {
			continue
		}

		// Check if it's a type name
		typeName, ok := obj.(*types.TypeName)
		if !ok {
			continue
		}

		// Check if the underlying type is an interface
		if iface, ok := typeName.Type().Underlying().(*types.Interface); ok {
			// Extract the interface definition
			ifaceStr := extractInterfaceDefinition(name, iface)
			ctx.Types[name] = ifaceStr
		}
	}
}

// collectReferencedTypesFromTypedef finds type names referenced in a type definition
func collectReferencedTypesFromTypedef(typeDef string, types map[string]bool) {
	// Simple heuristic: look for type names after field names
	// This is a basic implementation that looks for patterns like "repo UserRepository"
	lines := strings.Split(typeDef, "\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "//") {
			continue
		}

		// Look for field definitions
		parts := strings.Fields(line)
		if len(parts) >= 2 {
			// Last part is likely the type
			typeName := parts[len(parts)-1]
			// Remove pointer prefix if present
			typeName = strings.TrimPrefix(typeName, "*")
			// Skip if it looks like a builtin or qualified type
			if !strings.Contains(typeName, ".") && !isBuiltinType(typeName) {
				types[typeName] = true
			}
		}
	}
}

// extractInterfaceDefinition creates a string representation of an interface
func extractInterfaceDefinition(name string, iface *types.Interface) string {
	var result strings.Builder
	result.WriteString(name)
	result.WriteString(" interface {\n")

	// Add methods
	for i := 0; i < iface.NumMethods(); i++ {
		method := iface.Method(i)
		sig := method.Type().(*types.Signature)
		result.WriteString("\t")
		result.WriteString(method.Name())
		result.WriteString(formatMethodSignature(sig))
		result.WriteString("\n")
	}

	// Add embedded interfaces
	for i := 0; i < iface.NumEmbeddeds(); i++ {
		embedded := iface.EmbeddedType(i)
		result.WriteString("\t")
		result.WriteString(embedded.String())
		result.WriteString("\n")
	}

	result.WriteString("}")
	return result.String()
}

// formatMethodSignature formats a method signature for an interface
func formatMethodSignature(sig *types.Signature) string {
	var result strings.Builder
	result.WriteString("(")

	// Parameters
	params := sig.Params()
	for i := 0; i < params.Len(); i++ {
		if i > 0 {
			result.WriteString(", ")
		}
		param := params.At(i)
		if param.Name() != "" {
			result.WriteString(param.Name())
			result.WriteString(" ")
		}
		result.WriteString(param.Type().String())
	}

	result.WriteString(")")

	// Results
	results := sig.Results()
	if results.Len() > 0 {
		result.WriteString(" ")
		if results.Len() > 1 {
			result.WriteString("(")
		}
		for i := 0; i < results.Len(); i++ {
			if i > 0 {
				result.WriteString(", ")
			}
			res := results.At(i)
			if res.Name() != "" {
				result.WriteString(res.Name())
				result.WriteString(" ")
			}
			result.WriteString(res.Type().String())
		}
		if results.Len() > 1 {
			result.WriteString(")")
		}
	}

	return result.String()
}

// filterImportsByUsage filters imports based on what's actually used
func filterImportsByUsage(allImports map[string]string, target *parser.Target, ctx *RelevantContext, info *types.Info) []string {
	usedPackages := make(map[string]bool)

	// Check parameter and return types for package references
	for _, param := range target.Params {
		collectPackageFromType(param.Type, usedPackages)
	}
	for _, ret := range target.Returns {
		collectPackageFromType(ret.Type, usedPackages)
	}

	// Check types used in context
	for _, typeDef := range ctx.Types {
		// Look for qualified types in the definition
		collectPackagesFromString(typeDef, usedPackages)
	}

	// Check function signatures
	for _, funcSig := range ctx.Functions {
		collectPackagesFromString(funcSig, usedPackages)
	}

	// Build filtered import list
	var filteredImports []string
	for pkg := range usedPackages {
		if imp, exists := allImports[pkg]; exists {
			filteredImports = append(filteredImports, imp)
		}
	}

	return filteredImports
}

// collectPackageFromType extracts package name from qualified type
func collectPackageFromType(typeStr string, packages map[string]bool) {
	// Handle qualified types like context.Context
	if idx := strings.LastIndex(typeStr, "."); idx > 0 {
		pkg := typeStr[:idx]
		// Remove any prefix characters like *, [], etc.
		pkg = strings.TrimLeft(pkg, "*[]")
		if pkg != "" && !strings.Contains(pkg, " ") {
			packages[pkg] = true
		}
	}
}

// collectPackagesFromString finds package references in a string
func collectPackagesFromString(str string, packages map[string]bool) {
	// Common patterns for package types
	commonTypes := map[string]string{
		"context.Context": "context",
		"time.Time":       "time",
		"time.Duration":   "time",
		"error":           "", // builtin
	}

	for typeName, pkg := range commonTypes {
		if strings.Contains(str, typeName) && pkg != "" {
			packages[pkg] = true
		}
	}

	// Also look for any word.Word pattern that might be a qualified type
	parts := strings.Fields(str)
	for _, part := range parts {
		part = strings.TrimLeft(part, "*[]()")
		part = strings.TrimRight(part, ",;{})")
		if idx := strings.Index(part, "."); idx > 0 && idx < len(part)-1 {
			pkg := part[:idx]
			if isValidPackageName(pkg) {
				packages[pkg] = true
			}
		}
	}
}

// isValidPackageName checks if a string is a valid package name
func isValidPackageName(name string) bool {
	if name == "" || strings.ContainsAny(name, " \t\n{}()[]<>") {
		return false
	}
	// Basic check - package names are usually lowercase
	return strings.ToLower(name) == name
}
