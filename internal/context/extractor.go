package context

import (
	"go/ast"
	goparser "go/parser"
	"go/token"
	"strings"

	astutils "github.com/rail44/glyph/internal/ast"
	"github.com/rail44/glyph/internal/parser"
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

	// Extract imports
	for _, imp := range node.Imports {
		if imp.Name != nil {
			ctx.Imports = append(ctx.Imports, imp.Name.Name+" "+imp.Path.Value)
		} else {
			ctx.Imports = append(ctx.Imports, imp.Path.Value)
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
