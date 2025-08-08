package codegen

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

	"github.com/rail44/mantra/internal/analysis"
	"github.com/rail44/mantra/internal/checksum"
	"github.com/rail44/mantra/internal/imports"
	"github.com/rail44/mantra/internal/parser"
)

// Config holds configuration for code generation
type Config struct {
	Dest          string // Directory where generated files will be saved
	PackageName   string // Package name for generated files
	SourcePackage string // Original package name for import reference
}

type Generator struct {
	config *Config
}

func New(config *Config) *Generator {
	return &Generator{config: config}
}

// PrepareTargetStubs prepares the generated file with stub implementations for targets
// that are about to be generated. This creates a valid Go file that can be analyzed
// by go/packages while generation is in progress.
//
// For targets to be generated: uses panic("not implemented")
// For other targets: preserves existing implementation if file exists, otherwise uses panic
func (g *Generator) PrepareTargetStubs(fileInfo *parser.FileInfo, targetsToGenerate map[string]bool) error {
	// Create output directory if it doesn't exist
	if err := os.MkdirAll(g.config.Dest, 0755); err != nil {
		return fmt.Errorf("failed to create output directory: %w", err)
	}

	sourceFileName := filepath.Base(fileInfo.FilePath)
	outputFile := filepath.Join(g.config.Dest, sourceFileName)

	// Check if file already exists and preserve it if targets are already generated
	var existingContent string
	hasExistingFile := false
	if existingData, err := os.ReadFile(outputFile); err == nil {
		existingContent = string(existingData)
		hasExistingFile = true
	}

	// Build results for file generation
	var results []*parser.GenerationResult
	for _, target := range fileInfo.Targets {
		key := target.GetDisplayName()

		if targetsToGenerate[key] {
			// Target is about to be generated - use stub (panic)
			results = append(results, &parser.GenerationResult{
				Target:         target,
				Success:        false, // Marks it to use panic("not implemented")
				Implementation: "",
			})
		} else if hasExistingFile {
			// Try to preserve existing implementation from file
			// Mark as successful to preserve whatever is in the existing file
			results = append(results, &parser.GenerationResult{
				Target:         target,
				Success:        true,
				Implementation: "// Preserved from existing file",
			})
		} else {
			// No existing file and not being generated - keep as stub
			results = append(results, &parser.GenerationResult{
				Target:         target,
				Success:        false,
				Implementation: "",
			})
		}
	}

	// If we have an existing file and some implementations to preserve,
	// we need to be smarter about merging
	var content string
	var err error

	if hasExistingFile && len(results) > 0 {
		// Use existing content as base for targets not being regenerated
		content, err = g.mergeWithExisting(fileInfo, results, existingContent)
	} else {
		// Generate fresh content
		content, err = g.generateFileContent(fileInfo, results, "")
	}

	if err != nil {
		return fmt.Errorf("failed to generate file content: %w", err)
	}

	// Write to file
	if err := os.WriteFile(outputFile, []byte(content), 0644); err != nil {
		return fmt.Errorf("failed to write file: %w", err)
	}

	return nil
}

// mergeWithExisting intelligently merges new stubs with existing implementations
func (g *Generator) mergeWithExisting(fileInfo *parser.FileInfo, results []*parser.GenerationResult, existingContent string) (string, error) {
	// For targets marked as successful (to be preserved), we'll keep the existing file
	// For targets marked as failed (to be regenerated), we'll use panic stubs

	// Parse the existing file to extract implementations
	fset := token.NewFileSet()
	existingAST, err := goparser.ParseFile(fset, "", existingContent, goparser.ParseComments)
	if err != nil {
		// If we can't parse existing file, fall back to fresh generation
		return g.generateFileContent(fileInfo, results, "")
	}

	// Build a map of which targets to regenerate (use stubs)
	useStub := make(map[string]bool)
	for _, result := range results {
		if !result.Success {
			useStub[result.Target.GetDisplayName()] = true
		}
	}

	// Walk through the existing AST and replace only targets to be regenerated
	ast.Inspect(existingAST, func(n ast.Node) bool {
		if fn, ok := n.(*ast.FuncDecl); ok {
			funcName := fn.Name.Name
			if fn.Recv != nil && len(fn.Recv.List) > 0 {
				// Method - check if it needs stub
				for _, result := range results {
					if result.Target.Name == funcName && useStub[result.Target.GetDisplayName()] {
						// Replace with panic stub
						fn.Body = &ast.BlockStmt{
							List: []ast.Stmt{
								&ast.ExprStmt{
									X: &ast.CallExpr{
										Fun: &ast.Ident{Name: "panic"},
										Args: []ast.Expr{
											&ast.BasicLit{
												Kind:  token.STRING,
												Value: `"not implemented"`,
											},
										},
									},
								},
							},
						}
						break
					}
				}
			}
		}
		return true
	})

	// Format the modified AST back to source code
	var buf strings.Builder
	if err := format.Node(&buf, fset, existingAST); err != nil {
		// Fall back to fresh generation if formatting fails
		return g.generateFileContent(fileInfo, results, "")
	}

	return buf.String(), nil
}

// GenerateFile generates a complete file with implementations for all targets
func (g *Generator) GenerateFile(fileInfo *parser.FileInfo, results []*parser.GenerationResult) error {
	// Create output directory if it doesn't exist
	if err := os.MkdirAll(g.config.Dest, 0755); err != nil {
		return fmt.Errorf("failed to create output directory: %w", err)
	}

	// Check if generated file already exists
	sourceFileName := filepath.Base(fileInfo.FilePath)
	outputFile := filepath.Join(g.config.Dest, sourceFileName)

	var existingContent string
	if existingData, err := os.ReadFile(outputFile); err == nil {
		existingContent = string(existingData)
	}

	// Generate the file content
	content, err := g.generateFileContent(fileInfo, results, existingContent)
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

	// File paths already determined above

	// Write the generated file
	if err := os.WriteFile(outputFile, formatted, 0644); err != nil {
		return fmt.Errorf("failed to write file: %w", err)
	}

	return nil
}

// generateFileContent creates the content for the generated file by replacing mantra functions
func (g *Generator) generateFileContent(fileInfo *parser.FileInfo, results []*parser.GenerationResult, existingContent string) (string, error) {
	// Start with the original source content
	content := fileInfo.SourceContent

	// Add generated code header if not already present
	if !strings.Contains(content, "Code generated by mantra") {
		// Insert header after package declaration
		lines := strings.Split(content, "\n")
		for i, line := range lines {
			if strings.HasPrefix(strings.TrimSpace(line), "package ") {
				header := "// Code generated by mantra; DO NOT EDIT.\n"
				lines[i] = line + "\n\n" + header
				content = strings.Join(lines, "\n")
				break
			}
		}
	}

	// Change package name
	content = strings.Replace(content, fmt.Sprintf("package %s", fileInfo.PackageName), fmt.Sprintf("package %s", g.config.PackageName), 1)

	// Convert blank imports to regular imports
	content = g.convertBlankImports(content)

	// Create a map for quick lookup of results by target name
	resultMap := make(map[string]*parser.GenerationResult)
	for _, result := range results {
		resultMap[result.Target.Name] = result
	}

	// Sort targets by line number in reverse order to avoid line number shifts
	var targetsToProcess []*parser.Target
	for _, target := range fileInfo.Targets {
		// Process all targets with mantra comments
		if result, exists := resultMap[target.Name]; exists {
			if result.Success {
				target.Implementation = result.Implementation
				target.GenerationFailed = false
			} else {
				// Mark as failed, keep original implementation (panic), store failure reason
				target.GenerationFailed = true
				target.FailureReason = result.FailureReason
			}
		} else {
			// No result found - mark as failed
			target.GenerationFailed = true
			target.FailureReason = &parser.FailureReason{
				Phase:   "unknown",
				Message: "No generation result found for this target",
				Context: "Target may have been skipped during processing",
			}
		}
		targetsToProcess = append(targetsToProcess, target)
	}

	// Sort by start line in descending order (process from bottom to top)
	sort.Slice(targetsToProcess, func(i, j int) bool {
		iStartLine := targetsToProcess[i].TokenSet.Position(targetsToProcess[i].FuncDecl.Pos()).Line
		jStartLine := targetsToProcess[j].TokenSet.Position(targetsToProcess[j].FuncDecl.Pos()).Line
		return iStartLine > jStartLine // Descending order
	})

	// Replace all mantra functions with their implementations in a single AST pass
	newContent, err := g.replaceAllFunctionsWithChecksum(content, targetsToProcess, fileInfo.FilePath)
	if err != nil {
		return "", fmt.Errorf("failed to replace functions: %w", err)
	}
	content = newContent

	// Analyze required imports from successful implementations
	var requiredImports []string
	for _, result := range results {
		if result.Success {
			implImports := imports.AnalyzeRequiredImports(result.Implementation)
			requiredImports = imports.MergeImports(requiredImports, implImports)
		}
	}

	// Extract blank imports from the original file (imports marked with _)
	// These indicate packages that should be used in generated code
	blankImports := imports.ExtractBlankImports(fileInfo.SourceContent)
	if len(blankImports) > 0 {
		requiredImports = imports.MergeImports(requiredImports, blankImports)
	}

	// Add required imports to the generated file
	if len(requiredImports) > 0 {
		content = g.addImports(content, requiredImports)
	}

	return content, nil
}

// replaceAllFunctionsWithChecksum replaces all target functions and adds checksums
func (g *Generator) replaceAllFunctionsWithChecksum(content string, targets []*parser.Target, filePath string) (string, error) {
	if len(targets) == 0 {
		return content, nil
	}

	// Parse the original content as AST once
	fset := token.NewFileSet()
	node, err := goparser.ParseFile(fset, filePath, content, goparser.ParseComments)
	if err != nil {
		return "", fmt.Errorf("failed to parse file content: %w", err)
	}

	// targetData holds all data needed for replacing a target function
	type targetData struct {
		sourceTarget *parser.Target // Original source file's target
		implBody     *ast.BlockStmt
		checksum     string
	}

	// Prepare implementation bodies and checksums for all targets
	sourceTargetData := make(map[string]*targetData)

	for _, target := range targets {
		var implBody *ast.BlockStmt
		var checksumComment string

		if target.GenerationFailed {
			// For failed targets, keep original body and set detailed failure comment
			implBody = target.FuncDecl.Body // Keep original implementation (panic)
			if target.FailureReason != nil {
				checksumComment = fmt.Sprintf("// mantra:failed:%s: %s",
					target.FailureReason.Phase, target.FailureReason.Message)
			} else {
				checksumComment = "// mantra:failed: unknown reason"
			}
		} else {
			// Parse the implementation as a function body
			cleanedImpl := cleanCode(target.Implementation)
			var err error
			implBody, err = g.parseImplementationAsBlockWithFileSet(cleanedImpl, fset)
			if err != nil {
				return "", fmt.Errorf("failed to parse implementation for %s: %w", target.Name, err)
			}

			// Calculate checksum for the comment
			cs := checksum.Calculate(target)
			checksumComment = checksum.FormatComment(cs)
		}

		// Create a unique key for the target
		key := g.getTargetKey(target)
		sourceTargetData[key] = &targetData{
			sourceTarget: target,
			implBody:     implBody,
			checksum:     checksumComment,
		}
	}

	// Find and update all target functions in the AST in a single pass
	processedCount := 0
	ast.Inspect(node, func(n ast.Node) bool {
		if funcDecl, ok := n.(*ast.FuncDecl); ok {
			// Try to match this function with any of our targets
			for key, data := range sourceTargetData {
				if g.isTargetFunction(funcDecl, data.sourceTarget) {
					processedCount++

					// Replace function body with the new implementation
					funcDecl.Body = data.implBody

					// Remove old doc from file's Comments list if exists
					if funcDecl.Doc != nil {
						for i, cg := range node.Comments {
							if cg == funcDecl.Doc {
								node.Comments = append(node.Comments[:i], node.Comments[i+1:]...)
								break
							}
						}
					}

					// Build new comments: original + checksum
					var comments []*ast.Comment
					pos := funcDecl.Pos() - 1

					// Copy original comments from source
					if data.sourceTarget.FuncDecl.Doc != nil {
						for i, c := range data.sourceTarget.FuncDecl.Doc.List {
							comments = append(comments, &ast.Comment{
								Slash: pos - token.Pos(len(data.sourceTarget.FuncDecl.Doc.List)-i),
								Text:  c.Text,
							})
						}
					}

					// Add checksum
					comments = append(comments, &ast.Comment{
						Slash: pos,
						Text:  data.checksum,
					})

					// Create and set new doc
					newDoc := &ast.CommentGroup{List: comments}
					funcDecl.Doc = newDoc
					node.Comments = append(node.Comments, newDoc)

					// Remove from map to avoid processing again
					delete(sourceTargetData, key)
					break
				}
			}
		}
		return true
	})

	if processedCount != len(targets) {
		// List unprocessed functions for better debugging
		unprocessed := make([]string, 0, len(sourceTargetData))
		for key := range sourceTargetData {
			unprocessed = append(unprocessed, key)
		}
		return "", fmt.Errorf("expected to process %d functions but processed %d (unprocessed: %v)",
			len(targets), processedCount, unprocessed)
	}

	// Comments have been added to node.Comments during processing

	// Format the modified AST back to source code once
	var buf strings.Builder
	if err := format.Node(&buf, fset, node); err != nil {
		return "", fmt.Errorf("failed to format modified AST: %w", err)
	}

	return buf.String(), nil
}

// getTargetKey creates a unique key for a target function
func (g *Generator) getTargetKey(target *parser.Target) string {
	if target.Receiver != nil {
		return fmt.Sprintf("%s.%s", target.Receiver.Type, target.Name)
	}
	return target.Name
}

// isTargetFunction checks if the given function declaration matches the target
func (g *Generator) isTargetFunction(funcDecl *ast.FuncDecl, target *parser.Target) bool {
	// Check function name
	if funcDecl.Name.Name != target.Name {
		return false
	}

	// Check receiver for methods
	if target.Receiver != nil {
		if funcDecl.Recv == nil || len(funcDecl.Recv.List) == 0 {
			return false
		}

		receiverType := analysis.ExtractTypeString(funcDecl.Recv.List[0].Type)
		return receiverType == target.Receiver.Type
	}

	// For functions (no receiver), ensure funcDecl also has no receiver
	return funcDecl.Recv == nil
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

// cleanCode removes markdown formatting and extracts function body from AI responses.
// It handles cases where the AI includes function signatures or markdown code blocks.
func cleanCode(response string) string {
	response = strings.TrimSpace(response)

	// First, check if the response contains markdown code blocks
	if strings.Contains(response, "```") {
		// Find the content between ```go or ``` and the closing ```
		startMarkers := []string{"```go\n", "```go\r\n", "```\n", "```\r\n"}
		endMarker := "```"

		for _, startMarker := range startMarkers {
			startIdx := strings.Index(response, startMarker)
			if startIdx != -1 {
				// Found a code block, extract content
				startIdx += len(startMarker)
				endIdx := strings.Index(response[startIdx:], endMarker)
				if endIdx != -1 {
					response = response[startIdx : startIdx+endIdx]
					break
				}
			}
		}
	}

	// Remove any remaining markdown artifacts
	response = strings.TrimSpace(response)

	// Remove common explanatory prefixes
	explanatoryPrefixes := []string{
		"Here's the implementation:",
		"Here is the implementation:",
		"The implementation:",
		"Implementation:",
	}

	for _, prefix := range explanatoryPrefixes {
		if strings.HasPrefix(response, prefix) {
			response = strings.TrimPrefix(response, prefix)
			response = strings.TrimSpace(response)
		}
	}

	// Check if response contains function signature and extract body
	if strings.Contains(response, "func ") && strings.Contains(response, "{") {
		// Find the first opening brace
		braceIdx := strings.Index(response, "{")
		if braceIdx != -1 {
			// Find the matching closing brace
			braceCount := 1
			i := braceIdx + 1
			for i < len(response) && braceCount > 0 {
				switch response[i] {
				case '{':
					braceCount++
				case '}':
					braceCount--
				}
				i++
			}

			// Extract the body (excluding the braces)
			if braceCount == 0 && braceIdx+1 < i-1 {
				body := response[braceIdx+1 : i-1]
				// Remove leading/trailing whitespace but preserve internal indentation
				lines := strings.Split(body, "\n")
				if len(lines) > 0 {
					// Remove empty first and last lines
					if strings.TrimSpace(lines[0]) == "" && len(lines) > 1 {
						lines = lines[1:]
					}
					if len(lines) > 0 && strings.TrimSpace(lines[len(lines)-1]) == "" {
						lines = lines[:len(lines)-1]
					}
				}
				return strings.Join(lines, "\n")
			}
		}
	}

	return response
}
