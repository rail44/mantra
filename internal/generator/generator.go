package generator

import (
	"fmt"
	"go/ast"
	"go/format"
	goparser "go/parser"
	"go/token"
	"os"
	"path/filepath"
	"sort"
	"strings"

	astutils "github.com/rail44/mantra/internal/ast"
	"github.com/rail44/mantra/internal/imports"
	"github.com/rail44/mantra/internal/parser"
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

// generateFileContent creates the content for the generated file by replacing mantra functions
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
	sort.Slice(targetsToProcess, func(i, j int) bool {
		iStartLine := targetsToProcess[i].TokenSet.Position(targetsToProcess[i].FuncDecl.Pos()).Line
		jStartLine := targetsToProcess[j].TokenSet.Position(targetsToProcess[j].FuncDecl.Pos()).Line
		return iStartLine > jStartLine // Descending order
	})

	// Replace each mantra function with its implementation (from bottom to top)
	for _, target := range targetsToProcess {
		// Always use original file content and line numbers for each replacement
		newContent, err := g.replaceFunctionBody(content, target, target.Implementation)
		if err != nil {
			return "", fmt.Errorf("failed to replace function %s: %w", target.Name, err)
		}
		content = newContent
	}

	// Analyze required imports from all implementations
	var requiredImports []string
	for _, impl := range implementations {
		implImports := imports.AnalyzeRequiredImports(impl)
		requiredImports = imports.MergeImports(requiredImports, implImports)
	}

	// Add required imports to the generated file
	if len(requiredImports) > 0 {
		content = g.addImports(content, requiredImports)
	}

	return content, nil
}

// replaceFunctionBody replaces a function body with generated implementation using AST manipulation.
// It parses both the original content and the implementation with the same FileSet to ensure
// consistent position information and avoid formatting issues.
func (g *Generator) replaceFunctionBody(content string, target *parser.Target, implementation string) (string, error) {
	// Parse the original content as AST
	fset := token.NewFileSet()
	node, err := goparser.ParseFile(fset, target.FilePath, content, goparser.ParseComments)
	if err != nil {
		return "", fmt.Errorf("failed to parse file content: %w", err)
	}

	// Parse the implementation as a function body
	cleanedImpl := cleanCode(implementation)
	implBody, err := g.parseImplementationAsBlockWithFileSet(cleanedImpl, fset)
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
						receiverType := astutils.GetTypeString(funcDecl.Recv.List[0].Type)
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

// parseImplementationAsBlockWithFileSet parses implementation code as a block statement.
// It uses the provided FileSet to maintain position consistency with the original file.
func (g *Generator) parseImplementationAsBlockWithFileSet(implementation string, fset *token.FileSet) (*ast.BlockStmt, error) {
	// Wrap implementation in a function to parse as valid Go code
	testFunc := fmt.Sprintf("package main\nfunc test() {\n%s\n}", implementation)

	node, err := goparser.ParseFile(fset, "<generated-implementation>", testFunc, 0)
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

// cleanCode removes markdown formatting and extracts function body from AI responses.
// It handles cases where the AI includes function signatures or markdown code blocks.
func cleanCode(response string) string {
	response = strings.TrimSpace(response)

	// Remove markdown code blocks
	if strings.HasPrefix(response, "```") {
		lines := strings.Split(response, "\n")
		var cleaned []string
		inCodeBlock := false

		for _, line := range lines {
			if strings.HasPrefix(strings.TrimSpace(line), "```") {
				inCodeBlock = !inCodeBlock
				continue
			}
			if inCodeBlock || (!strings.HasPrefix(line, "```") && len(cleaned) > 0) {
				cleaned = append(cleaned, line)
			}
		}
		response = strings.Join(cleaned, "\n")
	}

	// Check if response contains function signature and extract body
	if strings.Contains(response, "func ") && strings.Contains(response, "{") {
		// Find the first opening brace
		braceIdx := strings.Index(response, "{")
		if braceIdx != -1 {
			// Find the last closing brace
			lastBrace := strings.LastIndex(response, "}")
			if lastBrace > braceIdx {
				// Extract only the body between braces
				response = response[braceIdx+1 : lastBrace]
			}
		}
	}

	response = strings.TrimSpace(response)
	return response
}
