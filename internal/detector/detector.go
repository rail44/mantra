package detector

import (
	"fmt"
	"go/ast"
	goparser "go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"

	"github.com/rail44/mantra/internal/checksum"
	"github.com/rail44/mantra/internal/parser"
)

// Status represents the generation status of a target
type Status int

const (
	StatusUngenerated Status = iota // Never generated
	StatusOutdated                  // Generated but declaration changed
	StatusCurrent                   // Generated and up-to-date
)

// FileDetectionResult represents detection results for a single file.
// It includes both the file information and any mantra targets found within it.
// Files without mantra targets will have an empty Statuses slice, but still
// need to be processed for copying to the destination directory.
type FileDetectionResult struct {
	FileInfo *parser.FileInfo
	Statuses []*TargetStatus // Empty if no mantra targets in file
}

// TargetStatus holds a target and its generation status
type TargetStatus struct {
	Target           *parser.Target
	Status           Status
	CurrentChecksum  string // Checksum of current declaration
	ExistingChecksum string // Checksum found in generated file (if any)
	ExistingImpl     string // Existing implementation (if checksum matches)
}

// DetectPackageTargets analyzes all Go files in a package directory and returns detection results for all files
func DetectPackageTargets(packageDir string, generatedDir string) ([]*FileDetectionResult, error) {
	// Find all Go files in the package
	files, err := filepath.Glob(filepath.Join(packageDir, "*.go"))
	if err != nil {
		return nil, fmt.Errorf("failed to glob files: %w", err)
	}

	var allResults []*FileDetectionResult

	// Process each source file
	for _, sourceFile := range files {
		// Skip test files
		if strings.HasSuffix(sourceFile, "_test.go") {
			continue
		}

		// Parse source file for targets
		fileInfo, err := parser.ParseFileInfo(sourceFile)
		if err != nil {
			return nil, fmt.Errorf("failed to parse %s: %w", sourceFile, err)
		}

		// Get generated file path
		generatedFile := filepath.Join(generatedDir, filepath.Base(sourceFile))

		// Load existing implementations from generated file (if exists)
		existingImplementations := make(map[string]*ImplementationInfo)
		if _, err := os.Stat(generatedFile); err == nil {
			impls, err := extractImplementationsFromFile(generatedFile)
			if err == nil {
				existingImplementations = impls
			}
		}

		// Create FileDetectionResult for this file
		fileResult := &FileDetectionResult{
			FileInfo: fileInfo,
			Statuses: []*TargetStatus{},
		}

		// Check status of each target
		for _, target := range fileInfo.Targets {
			// Process all targets with mantra comments (remove HasPanic check)
			currentChecksum := checksum.Calculate(target)
			existingImpl, exists := existingImplementations[target.Name]

			var status Status
			var existingChecksum string
			var existingBody string

			if exists {
				existingChecksum = existingImpl.Checksum
				if existingChecksum == currentChecksum {
					status = StatusCurrent
					existingBody = existingImpl.Body
				} else {
					status = StatusOutdated
				}
			} else {
				status = StatusUngenerated
			}

			fileResult.Statuses = append(fileResult.Statuses, &TargetStatus{
				Target:           target,
				Status:           status,
				CurrentChecksum:  currentChecksum,
				ExistingChecksum: existingChecksum,
				ExistingImpl:     existingBody,
			})
		}

		// Add file result even if it has no targets
		allResults = append(allResults, fileResult)
	}

	return allResults, nil
}

// ImplementationInfo holds checksum and implementation for a function
type ImplementationInfo struct {
	Checksum string
	Body     string
}

// extractImplementationsFromFile parses a generated file and extracts function checksums and implementations
func extractImplementationsFromFile(filePath string) (map[string]*ImplementationInfo, error) {
	content, err := os.ReadFile(filePath)
	if err != nil {
		return nil, err
	}

	fset := token.NewFileSet()
	node, err := goparser.ParseFile(fset, filePath, content, goparser.ParseComments)
	if err != nil {
		return nil, err
	}

	implementations := make(map[string]*ImplementationInfo)

	// Walk through all functions
	ast.Inspect(node, func(n ast.Node) bool {
		funcDecl, ok := n.(*ast.FuncDecl)
		if !ok {
			return true
		}

		// Look for checksum comment immediately before function
		funcPos := fset.Position(funcDecl.Pos())
		var foundChecksum string
		for _, commentGroup := range node.Comments {
			commentPos := fset.Position(commentGroup.End())
			// Check if comment is right before function (within 2 lines)
			if commentPos.Line >= funcPos.Line-2 && commentPos.Line < funcPos.Line {
				for _, comment := range commentGroup.List {
					if cs := checksum.ExtractFromComment(comment.Text); cs != "" {
						foundChecksum = cs
						break
					}
				}
			}
		}

		// If we found a checksum, extract the function body
		if foundChecksum != "" {
			// Get the function body without panic check
			bodyContent := extractFunctionBody(string(content), funcDecl, fset)
			implementations[funcDecl.Name.Name] = &ImplementationInfo{
				Checksum: foundChecksum,
				Body:     bodyContent,
			}
		}

		return true
	})

	return implementations, nil
}

// extractFunctionBody extracts the body content of a function from source
func extractFunctionBody(source string, funcDecl *ast.FuncDecl, fset *token.FileSet) string {
	if funcDecl.Body == nil {
		return ""
	}

	// Get positions
	start := fset.Position(funcDecl.Body.Lbrace)
	end := fset.Position(funcDecl.Body.Rbrace)

	lines := strings.Split(source, "\n")
	if start.Line <= 0 || end.Line > len(lines) {
		return ""
	}

	// Extract body content (excluding braces)
	var bodyLines []string
	for i := start.Line; i < end.Line-1; i++ {
		bodyLines = append(bodyLines, lines[i])
	}

	// Handle last line specially to exclude closing brace
	if end.Line-1 < len(lines) && end.Line > start.Line {
		lastLine := lines[end.Line-1]
		if end.Column > 1 {
			lastLine = lastLine[:end.Column-1]
		}
		lastLine = strings.TrimRight(lastLine, " \t}")
		if lastLine != "" {
			bodyLines = append(bodyLines, lastLine)
		}
	}

	return strings.Join(bodyLines, "\n")
}

// FilterTargetsToGenerate returns only targets that need generation (ungenerated or outdated)
func FilterTargetsToGenerate(statuses []*TargetStatus) []*parser.Target {
	var targets []*parser.Target
	for _, status := range statuses {
		if status.Status != StatusCurrent {
			targets = append(targets, status.Target)
		}
	}
	return targets
}
