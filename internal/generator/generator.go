package generator

import (
	"fmt"
	"go/ast"
	"go/format"
	goparser "go/parser"
	"go/token"
	"os"
	"strings"

	"github.com/rail44/glyph/internal/parser"
)

type Generator struct{}

func New() *Generator {
	return &Generator{}
}

// GenerateForTarget generates implementation for a specific target
func (g *Generator) GenerateForTarget(target *parser.Target, implementation string) error {
	// Read the original file
	content, err := os.ReadFile(target.FilePath)
	if err != nil {
		return fmt.Errorf("failed to read source file: %w", err)
	}

	// Clean the implementation
	implementation = cleanCode(implementation)

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

// replacePanicWithImplementation replaces panic("not implemented") with actual implementation
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

// cleanCode removes markdown formatting from AI response
func cleanCode(response string) string {
	response = strings.TrimSpace(response)
	
	// Remove ```go and ``` markers if present
	if strings.HasPrefix(response, "```go") {
		response = strings.TrimPrefix(response, "```go")
		response = strings.TrimSuffix(response, "```")
		response = strings.TrimSpace(response)
	} else if strings.HasPrefix(response, "```") {
		response = strings.TrimPrefix(response, "```")
		response = strings.TrimSuffix(response, "```")
		response = strings.TrimSpace(response)
	}
	
	return response
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