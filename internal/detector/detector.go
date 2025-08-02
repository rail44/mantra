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

// TargetStatus holds a target and its generation status
type TargetStatus struct {
	Target           *parser.Target
	Status           Status
	CurrentChecksum  string // Checksum of current declaration
	ExistingChecksum string // Checksum found in generated file (if any)
}

// DetectPackageTargets analyzes all Go files in a package directory and returns targets with their status
func DetectPackageTargets(packageDir string, generatedDir string) ([]*TargetStatus, error) {
	// Find all Go files in the package
	files, err := filepath.Glob(filepath.Join(packageDir, "*.go"))
	if err != nil {
		return nil, fmt.Errorf("failed to glob files: %w", err)
	}

	var allStatuses []*TargetStatus

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

		// Load existing checksums from generated file (if exists)
		existingChecksums := make(map[string]string)
		if _, err := os.Stat(generatedFile); err == nil {
			checksums, err := extractChecksumsFromFile(generatedFile)
			if err == nil {
				existingChecksums = checksums
			}
		}

		// Check status of each target
		for _, target := range fileInfo.Targets {
			// Only process targets with panic (unimplemented)
			if !target.HasPanic {
				continue
			}

			currentChecksum := checksum.Calculate(target)
			existingChecksum, exists := existingChecksums[target.Name]

			status := StatusUngenerated
			if exists {
				if existingChecksum == currentChecksum {
					status = StatusCurrent
				} else {
					status = StatusOutdated
				}
			}

			allStatuses = append(allStatuses, &TargetStatus{
				Target:           target,
				Status:           status,
				CurrentChecksum:  currentChecksum,
				ExistingChecksum: existingChecksum,
			})
		}
	}

	return allStatuses, nil
}

// extractChecksumsFromFile parses a generated file and extracts function checksums
func extractChecksumsFromFile(filePath string) (map[string]string, error) {
	content, err := os.ReadFile(filePath)
	if err != nil {
		return nil, err
	}

	fset := token.NewFileSet()
	node, err := goparser.ParseFile(fset, filePath, content, goparser.ParseComments)
	if err != nil {
		return nil, err
	}

	checksums := make(map[string]string)

	// Walk through all functions
	ast.Inspect(node, func(n ast.Node) bool {
		funcDecl, ok := n.(*ast.FuncDecl)
		if !ok {
			return true
		}

		// Look for checksum comment immediately before function
		funcPos := fset.Position(funcDecl.Pos())
		for _, commentGroup := range node.Comments {
			commentPos := fset.Position(commentGroup.End())
			// Check if comment is right before function (within 2 lines)
			if commentPos.Line >= funcPos.Line-2 && commentPos.Line < funcPos.Line {
				for _, comment := range commentGroup.List {
					if cs := checksum.ExtractFromComment(comment.Text); cs != "" {
						checksums[funcDecl.Name.Name] = cs
						break
					}
				}
			}
		}

		return true
	})

	return checksums, nil
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
