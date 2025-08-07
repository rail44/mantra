package context

import (
	"fmt"
	"go/ast"
	goparser "go/parser"
	"go/token"

	"github.com/rail44/mantra/internal/analysis"
	"github.com/rail44/mantra/internal/parser"
)

// RelevantContext holds context information relevant to a function
type RelevantContext struct {
	Imports     []*ImportInfo                    // All imports with structured information
	Types       map[string]string                // Type definitions (name -> definition)
	Methods     map[string][]analysis.MethodInfo // Type methods (typeName -> methods)
	PackageName string                           // Package name
}

// ExtractFunctionContext extracts context using a reliable, function-focused approach
func ExtractFunctionContext(filePath string, target *parser.Target) (*RelevantContext, error) {
	// Read and parse the source file
	fset := token.NewFileSet()
	node, err := goparser.ParseFile(fset, filePath, nil, goparser.ParseComments)
	if err != nil {
		return nil, fmt.Errorf("failed to parse file: %w", err)
	}

	ctx := &RelevantContext{
		Types:       make(map[string]string),
		Methods:     make(map[string][]analysis.MethodInfo),
		PackageName: node.Name.Name,
		Imports:     ExtractImportInfo(node),
	}

	// Step 1: Collect all type definitions in the file
	allTypes := collectAllTypeDefinitions(node, fset)

	// Step 2: Identify types directly referenced in function signature
	directlyUsedTypes := extractDirectlyUsedTypes(target)

	// Step 3: Add directly used types to context
	for typeName := range directlyUsedTypes {
		if typeDef, exists := allTypes[typeName]; exists {
			ctx.Types[typeName] = typeDef
		}
	}

	// Step 4: Recursively add types referenced by the included types
	for i := 0; i < 3; i++ { // Maximum 3 levels of recursion
		initialCount := len(ctx.Types)
		typesToCheck := make(map[string]string)
		for name, def := range ctx.Types {
			typesToCheck[name] = def
		}

		for _, typeDef := range typesToCheck {
			referencedTypes := analysis.ExtractReferencedTypesFromDefinition(typeDef)
			for refType := range referencedTypes {
				if _, exists := ctx.Types[refType]; !exists {
					if typeDef, exists := allTypes[refType]; exists {
						ctx.Types[refType] = typeDef
					}
				}
			}
		}
		// If no new types were added, stop recursion
		if len(ctx.Types) == initialCount {
			break
		}
	}

	// Step 5: Collect methods for included types
	for typeName := range ctx.Types {
		methods := collectMethodsForType(node, typeName)
		if len(methods) > 0 {
			ctx.Methods[typeName] = methods
		}
	}

	return ctx, nil
}

// collectAllTypeDefinitions collects all type definitions in the file
func collectAllTypeDefinitions(node *ast.File, fset *token.FileSet) map[string]string {
	types := make(map[string]string)

	for _, decl := range node.Decls {
		if genDecl, ok := decl.(*ast.GenDecl); ok {
			for _, spec := range genDecl.Specs {
				if typeSpec, ok := spec.(*ast.TypeSpec); ok {
					typeName := typeSpec.Name.Name
					typeDef := analysis.FormatTypeDefinition(typeSpec, fset)
					types[typeName] = typeDef
				}
			}
		}
	}

	return types
}

// extractDirectlyUsedTypes extracts types directly used in function signature
func extractDirectlyUsedTypes(target *parser.Target) map[string]bool {
	types := make(map[string]bool)

	// Add receiver type
	if target.Receiver != nil {
		typeName := analysis.CleanTypeName(target.Receiver.Type)
		if !analysis.IsBuiltinType(typeName) {
			types[typeName] = true
		}
	}

	// Add parameter types
	for _, param := range target.Params {
		typeName := analysis.CleanTypeName(param.Type)
		if !analysis.IsBuiltinType(typeName) {
			types[typeName] = true
		}
	}

	// Add return types
	for _, ret := range target.Returns {
		typeName := analysis.CleanTypeName(ret.Type)
		if !analysis.IsBuiltinType(typeName) {
			types[typeName] = true
		}
	}

	return types
}

// collectMethodsForType collects all methods for a given type
func collectMethodsForType(node *ast.File, typeName string) []analysis.MethodInfo {
	var methods []analysis.MethodInfo

	// Search for methods on this type
	for _, decl := range node.Decls {
		funcDecl, ok := decl.(*ast.FuncDecl)
		if !ok || funcDecl.Recv == nil || len(funcDecl.Recv.List) == 0 {
			continue
		}

		// Check if receiver matches our type
		recv := funcDecl.Recv.List[0]
		recvType := analysis.ExtractTypeString(recv.Type)
		// Handle both *TypeName and TypeName
		if recvType == typeName || recvType == "*"+typeName {
			method := analysis.MethodInfo{
				Name:      funcDecl.Name.Name,
				Signature: analysis.BuildFunctionSignatureFromDecl(funcDecl),
				Receiver:  recvType,
			}
			methods = append(methods, method)
		}
	}

	return methods
}
