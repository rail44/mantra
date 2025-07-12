package generator

import (
	"fmt"
	"go/ast"
	"go/format"
	goparser "go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"

	"github.com/rail44/glyph/internal/parser"
)

// Config holds configuration for code generation
type Config struct {
	OutputDir     string // Directory where generated files will be saved
	PackageName   string // Package name for generated files
	SourcePackage string // Original package name for import reference
}

type Generator struct {
	config *Config
}

func New(config *Config) *Generator {
	return &Generator{config: config}
}

// GenerateFile generates a complete file with implementations for all targets
func (g *Generator) GenerateFile(fileInfo *parser.FileInfo, implementations map[string]string) error {
	// Create output directory if it doesn't exist
	if err := os.MkdirAll(g.config.OutputDir, 0755); err != nil {
		return fmt.Errorf("failed to create output directory: %w", err)
	}

	// Generate the file content
	content, err := g.generateFileContent(fileInfo, implementations)
	if err != nil {
		return fmt.Errorf("failed to generate file content: %w", err)
	}

	// Format the Go code with more aggressive formatting
	formatted, err := g.formatCodeRobust(content)
	if err != nil {
		// If formatting fails, use the original code but log the error
		fmt.Fprintf(os.Stderr, "Warning: failed to format generated code: %v\n", err)
		formatted = []byte(content)
	}

	// Determine output file path
	sourceFileName := filepath.Base(fileInfo.FilePath)
	outputFile := filepath.Join(g.config.OutputDir, sourceFileName)

	// Write the generated file
	if err := os.WriteFile(outputFile, formatted, 0644); err != nil {
		return fmt.Errorf("failed to write file: %w", err)
	}

	return nil
}


// generateFileContent creates the content for the generated file by replacing glyph functions
func (g *Generator) generateFileContent(fileInfo *parser.FileInfo, implementations map[string]string) (string, error) {
	// Start with the original source content
	content := fileInfo.SourceContent
	
	// Change package name
	content = strings.Replace(content, fmt.Sprintf("package %s", fileInfo.PackageName), fmt.Sprintf("package %s", g.config.PackageName), 1)
	
	// Sort targets by line number in reverse order to avoid line number shifts
	var targetsToProcess []*parser.Target
	for _, target := range fileInfo.Targets {
		if target.HasPanic {
			if implementation, exists := implementations[target.Name]; exists {
				target.Implementation = implementation // Store implementation in target
				targetsToProcess = append(targetsToProcess, target)
			}
		}
	}
	
	// Sort by start line in descending order (process from bottom to top)
	for i := 0; i < len(targetsToProcess); i++ {
		for j := i + 1; j < len(targetsToProcess); j++ {
			iStartLine := targetsToProcess[i].TokenSet.Position(targetsToProcess[i].FuncDecl.Pos()).Line
			jStartLine := targetsToProcess[j].TokenSet.Position(targetsToProcess[j].FuncDecl.Pos()).Line
			if iStartLine < jStartLine {
				targetsToProcess[i], targetsToProcess[j] = targetsToProcess[j], targetsToProcess[i]
			}
		}
	}
	
	// Replace each glyph function with its implementation (from bottom to top)
	for _, target := range targetsToProcess {
		// Always use original file content and line numbers for each replacement
		newContent, err := g.replaceFunctionBody(content, target, target.Implementation)
		if err != nil {
			return "", fmt.Errorf("failed to replace function %s: %w", target.Name, err)
		}
		content = newContent
	}
	
	return content, nil
}

// replaceFunctionBody replaces a function body with generated implementation using AST
func (g *Generator) replaceFunctionBody(content string, target *parser.Target, implementation string) (string, error) {
	// Parse the original content as AST
	fset := token.NewFileSet()
	node, err := goparser.ParseFile(fset, target.FilePath, content, goparser.ParseComments)
	if err != nil {
		return "", fmt.Errorf("failed to parse file content: %w", err)
	}
	
	// Parse the implementation as a function body
	cleanedImpl := cleanCode(implementation)
	implBody, err := g.parseImplementationAsBlock(cleanedImpl)
	if err != nil {
		return "", fmt.Errorf("failed to parse implementation: %w", err)
	}
	
	// Find and replace the target function in the AST
	var targetFound bool
	ast.Inspect(node, func(n ast.Node) bool {
		if funcDecl, ok := n.(*ast.FuncDecl); ok {
			// Check if this is our target function (compare by name and receiver)
			if funcDecl.Name.Name == target.Name {
				// For methods, also check receiver type matches
				if target.Receiver != nil && funcDecl.Recv != nil {
					if len(funcDecl.Recv.List) > 0 {
						receiverType := g.getTypeString(funcDecl.Recv.List[0].Type)
						if receiverType == target.Receiver.Type {
							targetFound = true
						}
					}
				} else if target.Receiver == nil && funcDecl.Recv == nil {
					// Both are functions (no receiver)
					targetFound = true
				}
				
				if targetFound {
					// For methods, convert to function by adding receiver as first parameter
					if target.Receiver != nil {
						g.convertMethodToFunctionAST(funcDecl, target)
					}
					
					// Replace function body with the new implementation
					funcDecl.Body = implBody
					return false // Stop traversing this branch
				}
			}
		}
		return true
	})
	
	if !targetFound {
		return "", fmt.Errorf("target function %s not found in AST", target.Name)
	}
	
	// Format the modified AST back to source code
	var buf strings.Builder
	if err := format.Node(&buf, fset, node); err != nil {
		return "", fmt.Errorf("failed to format modified AST: %w", err)
	}
	
	// Don't apply formatting here - let the final stage handle it
	return buf.String(), nil
}

// parseImplementationAsBlock parses implementation code as a block statement
func (g *Generator) parseImplementationAsBlock(implementation string) (*ast.BlockStmt, error) {
	// Wrap implementation in a function to parse as valid Go code
	testFunc := fmt.Sprintf("package main\nfunc test() {\n%s\n}", implementation)
	
	fset := token.NewFileSet()
	node, err := goparser.ParseFile(fset, "temp.go", testFunc, 0)
	if err != nil {
		return nil, fmt.Errorf("implementation is not valid Go code: %w", err)
	}
	
	// Extract the function body
	if len(node.Decls) == 0 {
		return &ast.BlockStmt{}, nil
	}
	
	funcDecl, ok := node.Decls[0].(*ast.FuncDecl)
	if !ok || funcDecl.Body == nil {
		return &ast.BlockStmt{}, nil
	}
	
	return funcDecl.Body, nil
}

// convertMethodToFunctionAST converts a method AST node to a function by adding receiver as first parameter
func (g *Generator) convertMethodToFunctionAST(funcDecl *ast.FuncDecl, target *parser.Target) {
	if target.Receiver == nil || funcDecl.Recv == nil {
		return
	}
	
	// Get receiver information
	receiverField := funcDecl.Recv.List[0]
	receiverType := receiverField.Type
	receiverName := "self" // Default name
	if len(receiverField.Names) > 0 {
		receiverName = receiverField.Names[0].Name
	}
	
	// Create new parameter from receiver
	receiverParam := &ast.Field{
		Names: []*ast.Ident{ast.NewIdent(receiverName)},
		Type:  receiverType,
	}
	
	// Add receiver as first parameter
	if funcDecl.Type.Params == nil {
		funcDecl.Type.Params = &ast.FieldList{}
	}
	
	// Prepend receiver to parameter list
	newParams := []*ast.Field{receiverParam}
	newParams = append(newParams, funcDecl.Type.Params.List...)
	funcDecl.Type.Params.List = newParams
	
	// Remove receiver from function declaration
	funcDecl.Recv = nil
}

// getTypeString returns a string representation of an AST type expression
func (g *Generator) getTypeString(expr ast.Expr) string {
	switch t := expr.(type) {
	case *ast.Ident:
		return t.Name
	case *ast.ArrayType:
		return "[]" + g.getTypeString(t.Elt)
	case *ast.StarExpr:
		return "*" + g.getTypeString(t.X)
	case *ast.SelectorExpr:
		return g.getTypeString(t.X) + "." + t.Sel.Name
	case *ast.FuncType:
		return "func" // Simplified for now
	case *ast.InterfaceType:
		return "interface{}"
	case *ast.MapType:
		return "map[" + g.getTypeString(t.Key) + "]" + g.getTypeString(t.Value)
	case *ast.ChanType:
		return "chan " + g.getTypeString(t.Value)
	default:
		return "unknown"
	}
}





// cleanCode performs basic cleanup and validates the generated function
func cleanCode(response string) string {
	response = strings.TrimSpace(response)
	
	// Remove basic markdown if present
	response = strings.TrimPrefix(response, "```go")
	response = strings.TrimPrefix(response, "```")
	response = strings.TrimSuffix(response, "```")
	response = strings.TrimSpace(response)
	
	return response
}

// validateGeneratedFunction checks if the generated code is valid as a function body
func validateGeneratedFunction(code string) error {
	// Create a test function with the generated body
	testCode := fmt.Sprintf(`package main

func testFunc() {
%s
}`, code)
	
	// Try to parse as Go code
	fset := token.NewFileSet()
	_, err := goparser.ParseFile(fset, "test.go", testCode, goparser.ParseComments)
	if err != nil {
		return fmt.Errorf("generated code is not valid Go: %w", err)
	}
	
	return nil
}

// formatCodeRobust applies robust formatting to ensure clean output
func (g *Generator) formatCodeRobust(content string) ([]byte, error) {
	// Simply use format.Source - it follows go fmt standards
	return format.Source([]byte(content))
}

// collectNecessaryImports analyzes generated code to determine which imports are actually needed
func (g *Generator) collectNecessaryImports(fileInfo *parser.FileInfo, implementations map[string]string) []string {
	importSet := make(map[string]bool)
	
	// Analyze all generated implementations and function signatures to find used imports
	allGeneratedCode := strings.Builder{}
	for _, impl := range implementations {
		allGeneratedCode.WriteString(impl)
		allGeneratedCode.WriteString("\n")
	}
	
	// Also include function signatures to check for type references
	for _, target := range fileInfo.Targets {
		if target.Receiver != nil {
			allGeneratedCode.WriteString(target.Receiver.Type)
			allGeneratedCode.WriteString("\n")
		}
		for _, param := range target.Params {
			allGeneratedCode.WriteString(param.Type)
			allGeneratedCode.WriteString("\n")
		}
		for _, ret := range target.Returns {
			allGeneratedCode.WriteString(ret.Type)
			allGeneratedCode.WriteString("\n")
		}
	}
	
	generatedText := allGeneratedCode.String()
	
	// Check which of the original imports are actually used in generated code
	for _, imp := range fileInfo.Imports {
		if g.isImportUsed(generatedText, imp.Path) {
			importSet[imp.Path] = true
		}
	}
	
	// Check if source package is actually used in the generated functions
	if g.config.SourcePackage != "" && g.config.SourcePackage != g.config.PackageName && g.config.SourcePackage != "main" {
		if g.isImportUsed(generatedText, g.config.SourcePackage) {
			importSet[g.config.SourcePackage] = true
		}
	}
	
	// Convert set to sorted slice
	var imports []string
	for imp := range importSet {
		imports = append(imports, imp)
	}
	
	// Sort for consistent output
	for i := 0; i < len(imports); i++ {
		for j := i + 1; j < len(imports); j++ {
			if imports[i] > imports[j] {
				imports[i], imports[j] = imports[j], imports[i]
			}
		}
	}
	
	return imports
}

// isImportUsed checks if an import is used in the generated code
func (g *Generator) isImportUsed(code, importPath string) bool {
	// Extract package name from import path
	parts := strings.Split(importPath, "/")
	packageName := parts[len(parts)-1]
	
	// Handle special package names
	switch packageName {
	case "context":
		return strings.Contains(code, "context.") || strings.Contains(code, "Context")
	case "fmt":
		return strings.Contains(code, "fmt.") || strings.Contains(code, "Sprintf") || strings.Contains(code, "Printf") || strings.Contains(code, "Errorf")
	case "strings":
		return strings.Contains(code, "strings.")
	case "time":
		return strings.Contains(code, "time.") || strings.Contains(code, "Time")
	case "errors":
		return strings.Contains(code, "errors.")
	default:
		// For other packages, check if package name is used
		return strings.Contains(code, packageName+".")
	}
}

// UpdateImports adds necessary imports to the file
func (g *Generator) UpdateImports(filePath string, additionalImports []string) error {
	fset := token.NewFileSet()
	node, err := goparser.ParseFile(fset, filePath, nil, goparser.ParseComments)
	if err != nil {
		return fmt.Errorf("failed to parse file: %w", err)
	}

	// Track existing imports
	existingImports := make(map[string]bool)
	var importDecl *ast.GenDecl
	
	// Find import declaration
	for _, decl := range node.Decls {
		if genDecl, ok := decl.(*ast.GenDecl); ok && genDecl.Tok == token.IMPORT {
			importDecl = genDecl
			for _, spec := range genDecl.Specs {
				if importSpec, ok := spec.(*ast.ImportSpec); ok {
					path := strings.Trim(importSpec.Path.Value, `"`)
					existingImports[path] = true
				}
			}
			break
		}
	}

	// Add new imports if needed
	needsUpdate := false
	for _, imp := range additionalImports {
		if !existingImports[imp] {
			needsUpdate = true
			if importDecl != nil {
				// Add to existing import block
				importDecl.Specs = append(importDecl.Specs, &ast.ImportSpec{
					Path: &ast.BasicLit{
						Kind:  token.STRING,
						Value: fmt.Sprintf(`"%s"`, imp),
					},
				})
			}
		}
	}

	if needsUpdate {
		// Format and write back
		var buf strings.Builder
		if err := format.Node(&buf, fset, node); err != nil {
			return fmt.Errorf("failed to format AST: %w", err)
		}
		
		if err := os.WriteFile(filePath, []byte(buf.String()), 0644); err != nil {
			return fmt.Errorf("failed to write file: %w", err)
		}
	}

	return nil
}