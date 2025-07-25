package context

import (
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"strings"

	"golang.org/x/tools/go/packages"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/parser"
	"log/slog"
)

// ProjectContext holds context information from the entire project
type ProjectContext struct {
	RelevantContext
	PackageInfo *packages.Package
}

// ExtractProjectContext analyzes the project and extracts context relevant to the target function
func ExtractProjectContext(filePath string, target *parser.Target) (*ProjectContext, error) {
	// Load the package containing the target file
	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedSyntax | 
		      packages.NeedTypes | packages.NeedTypesInfo | packages.NeedImports,
		Tests: false,
	}
	
	// Use file= pattern to load specific file directly
	// This works with both go modules and standalone files
	pkgs, err := packages.Load(cfg, "file="+filePath)
	if err != nil {
		return nil, fmt.Errorf("failed to load package: %w", err)
	}
	
	if len(pkgs) == 0 {
		return nil, fmt.Errorf("no packages found for file %s", filePath)
	}
	
	// Take the first package (should be the one we want)
	pkg := pkgs[0]
	if packages.PrintErrors(pkgs) > 0 {
		return nil, fmt.Errorf("package has errors")
	}
	
	log.Debug("loaded package", 
		slog.String("name", pkg.Name),
		slog.Int("files", len(pkg.GoFiles)))
	
	// Create context
	ctx := &ProjectContext{
		RelevantContext: RelevantContext{
			Types:        make(map[string]string),
			Constants:    make(map[string]string),
			Variables:    make(map[string]string),
			Functions:    make(map[string]string),
			PackageName:  pkg.Name,
			ReceiverType: "",
		},
		PackageInfo: pkg,
	}
	
	// Collect all type names used in the target function
	usedTypes := collectUsedTypes(target)
	
	// Extract types from all files in the package
	for i, file := range pkg.Syntax {
		if i < len(pkg.CompiledGoFiles) {
			log.Trace("analyzing file", slog.String("file", pkg.CompiledGoFiles[i]))
		}
		extractTypesFromFile(file, pkg, usedTypes, ctx)
	}
	
	// Extract imports from the target file
	for _, file := range pkg.Syntax {
		// Find the file that contains our target
		for _, decl := range file.Decls {
			if fn, ok := decl.(*ast.FuncDecl); ok && fn.Name.Name == target.Name {
				// This is our file, extract imports
				ctx.Imports = extractImportsFromFile(file, pkg, target, &ctx.RelevantContext)
				break
			}
		}
	}
	
	// Add receiver type if it's a method
	if target.Receiver != nil && target.Receiver.Type != "" {
		receiverType := strings.TrimPrefix(target.Receiver.Type, "*")
		// The type should already be collected, but ensure it's there
		if _, exists := ctx.Types[receiverType]; !exists {
			log.Warn("receiver type not found", slog.String("type", receiverType))
		}
	}
	
	return ctx, nil
}

// extractTypesFromFile extracts relevant types from a single file
func extractTypesFromFile(file *ast.File, pkg *packages.Package, usedTypes map[string]bool, ctx *ProjectContext) {
	for _, decl := range file.Decls {
		switch d := decl.(type) {
		case *ast.GenDecl:
			for _, spec := range d.Specs {
				switch s := spec.(type) {
				case *ast.TypeSpec:
					typeName := s.Name.Name
					
					// Check if this type is used or referenced
					if usedTypes[typeName] || isReferencedType(typeName, usedTypes, s.Type, pkg) {
						ctx.Types[typeName] = extractFullTypeDefinition(s, pkg)
						
						// If it's a struct, check for fields that reference other types
						if structType, ok := s.Type.(*ast.StructType); ok {
							collectTypesFromStruct(structType, pkg, usedTypes)
						}
					}
					
				case *ast.ValueSpec:
					// Extract constants and variables if needed
					for _, name := range s.Names {
						if d.Tok == token.CONST {
							ctx.Constants[name.Name] = extractValueDef(s, pkg)
						} else if d.Tok == token.VAR {
							ctx.Variables[name.Name] = extractValueDef(s, pkg)
						}
					}
				}
			}
			
		case *ast.FuncDecl:
			// Extract function signatures that might be useful
			if d.Name != nil {
				sig := extractFuncSignature(d, pkg)
				ctx.Functions[d.Name.Name] = sig
			}
		}
	}
}

// isReferencedType checks if a type is referenced by already collected types
func isReferencedType(typeName string, usedTypes map[string]bool, typeExpr ast.Expr, pkg *packages.Package) bool {
	// Check if this type is an interface that's referenced
	if _, ok := typeExpr.(*ast.InterfaceType); ok {
		// Check if any collected type references this interface
		for usedType := range usedTypes {
			if obj := pkg.Types.Scope().Lookup(usedType); obj != nil {
				if named, ok := obj.Type().(*types.Named); ok {
					if strct, ok := named.Underlying().(*types.Struct); ok {
						// Check struct fields
						for i := 0; i < strct.NumFields(); i++ {
							field := strct.Field(i)
							if field.Type().String() == typeName {
								return true
							}
						}
					}
				}
			}
		}
	}
	
	return false
}

// collectTypesFromStruct finds types referenced in struct fields
func collectTypesFromStruct(structType *ast.StructType, pkg *packages.Package, usedTypes map[string]bool) {
	if structType.Fields == nil {
		return
	}
	
	for _, field := range structType.Fields.List {
		if field.Type != nil {
			typeName := extractTypeNameFromExpr(field.Type)
			if typeName != "" && !isBuiltinType(typeName) {
				usedTypes[typeName] = true
			}
		}
	}
}

// extractTypeNameFromExpr extracts type name from an expression
func extractTypeNameFromExpr(expr ast.Expr) string {
	switch t := expr.(type) {
	case *ast.Ident:
		return t.Name
	case *ast.StarExpr:
		return extractTypeNameFromExpr(t.X)
	case *ast.ArrayType:
		return extractTypeNameFromExpr(t.Elt)
	case *ast.SelectorExpr:
		// For qualified types like pkg.Type
		return ""
	default:
		return ""
	}
}

// extractFullTypeDefinition extracts complete type definition with go/types info
func extractFullTypeDefinition(spec *ast.TypeSpec, pkg *packages.Package) string {
	// Get the type object
	obj := pkg.Types.Scope().Lookup(spec.Name.Name)
	if obj == nil {
		return formatTypeSpec(spec)
	}
	
	// If it's an interface, use our interface extraction
	if iface, ok := obj.Type().Underlying().(*types.Interface); ok {
		return extractInterfaceDefinition(spec.Name.Name, iface)
	}
	
	// Otherwise, use the AST representation
	return formatTypeSpec(spec)
}

// formatTypeSpec formats a type specification
func formatTypeSpec(spec *ast.TypeSpec) string {
	var result strings.Builder
	result.WriteString(spec.Name.Name)
	result.WriteString(" ")
	result.WriteString(formatTypeExpr(spec.Type))
	return result.String()
}

// formatTypeExpr formats a type expression
func formatTypeExpr(expr ast.Expr) string {
	switch t := expr.(type) {
	case *ast.StructType:
		return formatStructType(t)
	case *ast.InterfaceType:
		return formatInterfaceType(t)
	case *ast.Ident:
		return t.Name
	case *ast.StarExpr:
		return "*" + formatTypeExpr(t.X)
	case *ast.ArrayType:
		if t.Len == nil {
			return "[]" + formatTypeExpr(t.Elt)
		}
		return "[" + formatExprProj(t.Len) + "]" + formatTypeExpr(t.Elt)
	case *ast.MapType:
		return "map[" + formatTypeExpr(t.Key) + "]" + formatTypeExpr(t.Value)
	default:
		return "interface{}"
	}
}

// formatStructType formats a struct type
func formatStructType(s *ast.StructType) string {
	var result strings.Builder
	result.WriteString("struct {\n")
	
	if s.Fields != nil {
		for _, field := range s.Fields.List {
			result.WriteString("\t")
			
			// Field names
			if len(field.Names) > 0 {
				for i, name := range field.Names {
					if i > 0 {
						result.WriteString(", ")
					}
					result.WriteString(name.Name)
				}
				result.WriteString(" ")
			}
			
			// Field type
			result.WriteString(formatTypeExpr(field.Type))
			
			// Field tag
			if field.Tag != nil {
				result.WriteString(" ")
				result.WriteString(field.Tag.Value)
			}
			
			result.WriteString("\n")
		}
	}
	
	result.WriteString("}")
	return result.String()
}

// formatInterfaceType formats an interface type
func formatInterfaceType(i *ast.InterfaceType) string {
	var result strings.Builder
	result.WriteString("interface {\n")
	
	if i.Methods != nil {
		for _, method := range i.Methods.List {
			result.WriteString("\t")
			
			// Method name
			if len(method.Names) > 0 {
				result.WriteString(method.Names[0].Name)
			}
			
			// Method signature
			if fn, ok := method.Type.(*ast.FuncType); ok {
				result.WriteString(formatFuncTypeProj(fn))
			}
			
			result.WriteString("\n")
		}
	}
	
	result.WriteString("}")
	return result.String()
}

// formatFuncTypeProj formats a function type
func formatFuncTypeProj(fn *ast.FuncType) string {
	var result strings.Builder
	result.WriteString("(")
	
	// Parameters
	if fn.Params != nil {
		for i, field := range fn.Params.List {
			if i > 0 {
				result.WriteString(", ")
			}
			
			if len(field.Names) > 0 {
				for j, name := range field.Names {
					if j > 0 {
						result.WriteString(", ")
					}
					result.WriteString(name.Name)
					result.WriteString(" ")
				}
			}
			
			result.WriteString(formatTypeExpr(field.Type))
		}
	}
	
	result.WriteString(")")
	
	// Results
	if fn.Results != nil && len(fn.Results.List) > 0 {
		result.WriteString(" ")
		
		if len(fn.Results.List) > 1 {
			result.WriteString("(")
		}
		
		for i, field := range fn.Results.List {
			if i > 0 {
				result.WriteString(", ")
			}
			
			if len(field.Names) > 0 {
				for j, name := range field.Names {
					if j > 0 {
						result.WriteString(", ")
					}
					result.WriteString(name.Name)
					result.WriteString(" ")
				}
			}
			
			result.WriteString(formatTypeExpr(field.Type))
		}
		
		if len(fn.Results.List) > 1 {
			result.WriteString(")")
		}
	}
	
	return result.String()
}

// formatExprProj formats a general expression
func formatExprProj(expr ast.Expr) string {
	switch e := expr.(type) {
	case *ast.BasicLit:
		return e.Value
	case *ast.Ident:
		return e.Name
	default:
		return ""
	}
}

// extractValueDef extracts value definition
func extractValueDef(spec *ast.ValueSpec, pkg *packages.Package) string {
	// Simple implementation - can be enhanced
	var result strings.Builder
	
	for i, name := range spec.Names {
		if i > 0 {
			result.WriteString(", ")
		}
		result.WriteString(name.Name)
	}
	
	if spec.Type != nil {
		result.WriteString(" ")
		result.WriteString(formatTypeExpr(spec.Type))
	}
	
	if spec.Values != nil && len(spec.Values) > 0 {
		result.WriteString(" = ")
		// Simplified - just show that there's a value
		result.WriteString("...")
	}
	
	return result.String()
}

// extractFuncSignature extracts function signature
func extractFuncSignature(fn *ast.FuncDecl, pkg *packages.Package) string {
	var sig strings.Builder
	sig.WriteString("func ")
	
	// Receiver
	if fn.Recv != nil && len(fn.Recv.List) > 0 {
		sig.WriteString("(")
		recv := fn.Recv.List[0]
		if len(recv.Names) > 0 {
			sig.WriteString(recv.Names[0].Name)
			sig.WriteString(" ")
		}
		sig.WriteString(formatTypeExpr(recv.Type))
		sig.WriteString(") ")
	}
	
	// Name and signature
	sig.WriteString(fn.Name.Name)
	sig.WriteString(formatFuncTypeProj(fn.Type))
	
	return sig.String()
}

// extractImportsFromFile extracts imports used by the target function
func extractImportsFromFile(file *ast.File, pkg *packages.Package, target *parser.Target, ctx *RelevantContext) []string {
	// Collect all imports from the file
	allImports := make(map[string]string)
	for _, imp := range file.Imports {
		path := strings.Trim(imp.Path.Value, `"`)
		if imp.Name != nil {
			allImports[path] = imp.Name.Name + " " + imp.Path.Value
		} else {
			allImports[path] = imp.Path.Value
		}
	}
	
	// Use the existing filterImportsByUsage function
	return filterImportsByUsage(allImports, target, ctx, nil)
}