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

// replaceFunctionBody replaces a function body with generated implementation
func (g *Generator) replaceFunctionBody(content string, target *parser.Target, implementation string) (string, error) {
	lines := strings.Split(content, "\n")
	
	// Get function boundaries from AST
	startLine := target.TokenSet.Position(target.FuncDecl.Pos()).Line - 1 // Convert to 0-indexed
	endLine := target.TokenSet.Position(target.FuncDecl.End()).Line - 1   // Convert to 0-indexed
	
	if startLine < 0 || endLine >= len(lines) || startLine > endLine {
		return "", fmt.Errorf("invalid function boundaries for %s", target.Name)
	}
	
	// For methods, convert to function by adding receiver as first parameter
	if target.Receiver != nil {
		// Find the function signature line and modify it
		for i := startLine; i <= endLine; i++ {
			line := lines[i]
			if strings.Contains(line, "func") && strings.Contains(line, target.Name) {
				// Convert method to function
				newSignature := g.convertMethodToFunction(line, target)
				lines[i] = newSignature
				break
			}
		}
	}
	
	// Find the opening brace and replace function body
	openBraceIndex := -1
	closeBraceIndex := -1
	braceCount := 0
	
	for i := startLine; i <= endLine; i++ {
		line := lines[i]
		
		// Look for opening brace
		if openBraceIndex == -1 && strings.Contains(line, "{") {
			openBraceIndex = i
		}
		
		// Count braces to find the matching closing brace
		if openBraceIndex != -1 {
			for _, ch := range line {
				if ch == '{' {
					braceCount++
				} else if ch == '}' {
					braceCount--
					if braceCount == 0 {
						closeBraceIndex = i
						break
					}
				}
			}
		}
		
		if closeBraceIndex != -1 {
			break
		}
	}
	
	if openBraceIndex == -1 || closeBraceIndex == -1 {
		startLineNum := target.TokenSet.Position(target.FuncDecl.Pos()).Line
		endLineNum := target.TokenSet.Position(target.FuncDecl.End()).Line
		return "", fmt.Errorf("could not find function body boundaries for %s (lines %d-%d, open: %d, close: %d)", 
			target.Name, startLineNum, endLineNum, openBraceIndex, closeBraceIndex)
	}
	
	// Clean the implementation and add proper indentation
	cleanedImpl := cleanCode(implementation)
	baseIndent := getIndentation(lines[openBraceIndex+1])
	indentedImpl := indentCode(cleanedImpl, baseIndent)
	
	// Build new content
	var newLines []string
	
	// Add lines before function body
	newLines = append(newLines, lines[:openBraceIndex+1]...)
	
	// Add generated implementation
	if strings.TrimSpace(indentedImpl) != "" {
		implLines := strings.Split(indentedImpl, "\n")
		newLines = append(newLines, implLines...)
	}
	
	// Add closing brace and lines after
	newLines = append(newLines, lines[closeBraceIndex:]...)
	
	return strings.Join(newLines, "\n"), nil
}

// convertMethodToFunction converts a method signature to a function signature
func (g *Generator) convertMethodToFunction(line string, target *parser.Target) string {
	// Example: func (c *Calculator) Add(a, b float64) float64 {
	// Becomes: func Add(c *Calculator, a, b float64) float64 {
	
	// Find the receiver part
	receiverStart := strings.Index(line, "(")
	receiverEnd := strings.Index(line, ")") + 1
	funcStart := strings.Index(line[receiverEnd:], "func")
	if funcStart == -1 {
		funcStart = strings.Index(line, "func")
	} else {
		funcStart += receiverEnd
	}
	
	if receiverStart == -1 || receiverEnd == -1 || funcStart == -1 {
		return line // Can't parse, return original
	}
	
	// Extract receiver
	receiverType := target.Receiver.Type
	receiverName := target.Receiver.Name
	if receiverName == "" {
		receiverName = strings.ToLower(string(receiverType[0]))
	}
	
	// Handle package prefix for receiver type
	if g.config.SourcePackage != "main" && g.config.SourcePackage != "" {
		if strings.HasPrefix(receiverType, "*") {
			receiverType = "*" + g.config.SourcePackage + "." + strings.TrimPrefix(receiverType, "*")
		} else {
			receiverType = g.config.SourcePackage + "." + receiverType
		}
	}
	
	// Find function name and parameters
	funcNameStart := strings.Index(line[funcStart:], target.Name)
	if funcNameStart == -1 {
		return line // Can't find function name
	}
	funcNameStart += funcStart
	
	paramStart := strings.Index(line[funcNameStart:], "(")
	if paramStart == -1 {
		return line // Can't find parameters
	}
	paramStart += funcNameStart
	
	// Build new signature
	before := line[:funcStart]
	funcKeyword := "func "
	funcName := target.Name
	
	// Get the parameter part and add receiver as first parameter
	afterParams := line[paramStart:]
	if strings.HasPrefix(afterParams, "()") {
		// No parameters, add receiver only
		newParams := fmt.Sprintf("(%s %s)", receiverName, receiverType)
		afterParams = strings.Replace(afterParams, "()", newParams, 1)
	} else {
		// Has parameters, add receiver as first parameter
		closeParenIndex := findMatchingParen(afterParams)
		if closeParenIndex != -1 {
			existingParams := afterParams[1:closeParenIndex]
			newParams := fmt.Sprintf("(%s %s, %s)", receiverName, receiverType, existingParams)
			afterParams = newParams + afterParams[closeParenIndex+1:]
		}
	}
	
	return before + funcKeyword + funcName + afterParams
}

// findMatchingParen finds the matching closing parenthesis
func findMatchingParen(s string) int {
	if len(s) == 0 || s[0] != '(' {
		return -1
	}
	
	count := 0
	for i, ch := range s {
		if ch == '(' {
			count++
		} else if ch == ')' {
			count--
			if count == 0 {
				return i
			}
		}
	}
	return -1
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
		targetStartLine := target.TokenSet.Position(target.FuncDecl.Pos()).Line
		if lineNum == targetStartLine {
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