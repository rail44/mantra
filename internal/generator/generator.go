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

	// Format the Go code
	formatted, err := format.Source([]byte(content))
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

// GenerateForTarget generates implementation for a specific target (backwards compatibility)
func (g *Generator) GenerateForTarget(target *parser.Target, implementation string) error {
	// This method is kept for backwards compatibility but will use the old approach
	// It should be replaced with GenerateFile in the command layer
	
	// Read the original file
	content, err := os.ReadFile(target.FilePath)
	if err != nil {
		return fmt.Errorf("failed to read source file: %w", err)
	}

	// Clean the implementation
	implementation = cleanCode(implementation)

	// Validate the generated function
	if err := validateGeneratedFunction(implementation); err != nil {
		return fmt.Errorf("generated code validation failed: %w", err)
	}

	// Replace the panic statement with the implementation
	newContent, err := replacePanicWithImplementation(string(content), target, implementation)
	if err != nil {
		return fmt.Errorf("failed to replace panic: %w", err)
	}

	// Format the Go code
	formatted, err := format.Source([]byte(newContent))
	if err != nil {
		// If formatting fails, use the original code but log the error
		fmt.Fprintf(os.Stderr, "Warning: failed to format generated code: %v\n", err)
		formatted = []byte(newContent)
	}

	// Write the file back
	if err := os.WriteFile(target.FilePath, formatted, 0644); err != nil {
		return fmt.Errorf("failed to write file: %w", err)
	}

	return nil
}

// generateFileContent creates the content for the generated file
func (g *Generator) generateFileContent(fileInfo *parser.FileInfo, implementations map[string]string) (string, error) {
	var content strings.Builder
	
	// Package declaration
	content.WriteString(fmt.Sprintf("package %s\n\n", g.config.PackageName))
	
	// Imports
	content.WriteString("import (\n")
	
	// Add source package import if needed (only if it's not "main" and not the same as current package)
	if g.config.SourcePackage != "" && g.config.SourcePackage != g.config.PackageName && g.config.SourcePackage != "main" {
		content.WriteString(fmt.Sprintf("\t\"%s\"\n", g.config.SourcePackage))
	}
	
	// Add original imports that might be needed
	for _, imp := range fileInfo.Imports {
		content.WriteString(fmt.Sprintf("\t\"%s\"\n", imp.Path))
	}
	
	// Add fmt import since it's commonly needed for error messages
	content.WriteString("\t\"fmt\"\n")
	
	content.WriteString(")\n\n")
	
	// Generate functions for each target
	for _, target := range fileInfo.Targets {
		if !target.HasPanic {
			continue // Skip targets without panic statements
		}
		
		implementation, exists := implementations[target.Name]
		if !exists {
			continue // Skip if no implementation provided
		}
		
		functionCode, err := g.generateFunction(target, implementation)
		if err != nil {
			return "", fmt.Errorf("failed to generate function %s: %w", target.Name, err)
		}
		
		content.WriteString(functionCode)
		content.WriteString("\n\n")
	}
	
	return content.String(), nil
}

// generateFunction creates a function from a target and implementation
func (g *Generator) generateFunction(target *parser.Target, implementation string) (string, error) {
	var function strings.Builder
	
	// Function signature
	function.WriteString("func ")
	function.WriteString(target.Name)
	function.WriteString("(")
	
	// Add receiver as first parameter if it's a method
	if target.Receiver != nil {
		receiverType := target.Receiver.Type
		// If source package is main, don't prefix types
		if g.config.SourcePackage != "main" && g.config.SourcePackage != "" {
			// Remove pointer prefix, add package prefix, then add pointer back if needed
			if strings.HasPrefix(receiverType, "*") {
				receiverType = "*" + g.config.SourcePackage + "." + strings.TrimPrefix(receiverType, "*")
			} else {
				receiverType = g.config.SourcePackage + "." + receiverType
			}
		}
		function.WriteString(fmt.Sprintf("%s %s", target.Receiver.Name, receiverType))
		if len(target.Params) > 0 {
			function.WriteString(", ")
		}
	}
	
	// Add parameters
	for i, param := range target.Params {
		if i > 0 {
			function.WriteString(", ")
		}
		if param.Name != "" {
			function.WriteString(param.Name + " ")
		}
		function.WriteString(g.convertTypeReference(param.Type))
	}
	
	function.WriteString(")")
	
	// Add return values
	if len(target.Returns) > 0 {
		function.WriteString(" ")
		if len(target.Returns) > 1 {
			function.WriteString("(")
		}
		for i, ret := range target.Returns {
			if i > 0 {
				function.WriteString(", ")
			}
			function.WriteString(g.convertTypeReference(ret.Type))
		}
		if len(target.Returns) > 1 {
			function.WriteString(")")
		}
	}
	
	function.WriteString(" {\n")
	
	// Clean and add implementation
	cleanedImpl := cleanCode(implementation)
	// Indent the implementation
	for _, line := range strings.Split(cleanedImpl, "\n") {
		if strings.TrimSpace(line) != "" {
			function.WriteString("\t" + line + "\n")
		} else {
			function.WriteString("\n")
		}
	}
	
	function.WriteString("}")
	
	return function.String(), nil
}

// convertTypeReference converts type references to include source package prefix where needed
func (g *Generator) convertTypeReference(typeStr string) string {
	// Don't prefix types if source package is main
	if g.config.SourcePackage == "main" {
		return typeStr
	}
	
	// Handle common types that need source package prefix
	// This is a simplified implementation - more sophisticated type analysis would be needed for production
	if g.config.SourcePackage != "" && !strings.Contains(typeStr, ".") {
		// Check if this looks like a custom type (starts with uppercase)
		if len(typeStr) > 0 && typeStr[0] >= 'A' && typeStr[0] <= 'Z' {
			// Don't prefix built-in types
			builtinTypes := map[string]bool{
				"string": true, "int": true, "int32": true, "int64": true,
				"float32": true, "float64": true, "bool": true,
				"byte": true, "rune": true, "error": true,
			}
			if !builtinTypes[typeStr] {
				return g.config.SourcePackage + "." + typeStr
			}
		}
	}
	return typeStr
}

// replacePanicWithImplementation replaces the entire function with the AI-generated implementation
func replacePanicWithImplementation(content string, target *parser.Target, implementation string) (string, error) {
	lines := strings.Split(content, "\n")
	
	// Find the function in the content
	functionStart := -1
	functionEnd := -1
	braceCount := 0
	inFunction := false
	
	for i, line := range lines {
		lineNum := i + 1
		
		// Check if we're at the start of our target function
		if lineNum == target.StartLine {
			functionStart = i
			inFunction = true
		}
		
		// Count braces if we're in the function
		if inFunction {
			for _, ch := range line {
				if ch == '{' {
					braceCount++
				} else if ch == '}' {
					braceCount--
					if braceCount == 0 {
						functionEnd = i
						inFunction = false
						break
					}
				}
			}
		}
		
		if functionEnd != -1 {
			break
		}
	}
	
	if functionStart == -1 || functionEnd == -1 {
		return "", fmt.Errorf("could not find function boundaries")
	}
	
	// Find the opening brace of the function
	openBraceIndex := -1
	for i := functionStart; i <= functionEnd; i++ {
		if strings.Contains(lines[i], "{") {
			openBraceIndex = i
			break
		}
	}
	
	if openBraceIndex == -1 {
		return "", fmt.Errorf("could not find function opening brace")
	}
	
	// Prepare the new implementation with proper indentation
	baseIndent := getIndentation(lines[openBraceIndex+1])
	indentedImpl := indentCode(implementation, baseIndent)
	
	// Build the new content
	var newLines []string
	
	// Add lines before the function
	newLines = append(newLines, lines[:openBraceIndex+1]...)
	
	// Add the new implementation
	implLines := strings.Split(indentedImpl, "\n")
	newLines = append(newLines, implLines...)
	
	// Add the closing brace
	newLines = append(newLines, lines[functionEnd])
	
	// Add lines after the function
	if functionEnd+1 < len(lines) {
		newLines = append(newLines, lines[functionEnd+1:]...)
	}
	
	return strings.Join(newLines, "\n"), nil
}

// getIndentation extracts the indentation from a line
func getIndentation(line string) string {
	indent := ""
	for _, ch := range line {
		if ch == ' ' || ch == '\t' {
			indent += string(ch)
		} else {
			break
		}
	}
	return indent
}

// indentCode adds indentation to each line of code
func indentCode(code, indent string) string {
	lines := strings.Split(strings.TrimSpace(code), "\n")
	for i, line := range lines {
		if line != "" {
			lines[i] = indent + line
		}
	}
	return strings.Join(lines, "\n")
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

// validateGeneratedFunction checks if the generated code is a valid Go function
func validateGeneratedFunction(code string) error {
	// Add temporary package declaration for parsing
	testCode := "package main\n\n" + code
	
	// Try to parse as Go code
	fset := token.NewFileSet()
	_, err := goparser.ParseFile(fset, "test.go", testCode, goparser.ParseComments)
	if err != nil {
		return fmt.Errorf("generated code is not valid Go: %w", err)
	}
	
	// Check if it contains at least one function
	if !strings.Contains(code, "func ") {
		return fmt.Errorf("generated code does not contain a function")
	}
	
	return nil
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