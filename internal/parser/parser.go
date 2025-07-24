package parser

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"strings"

	astutils "github.com/rail44/mantra/internal/ast"
)

const (
	// maxCommentGap is the maximum allowed gap between a mantra comment and its target function
	// This prevents associating comments that are too far from the function
	maxCommentGap = 50
)

// FileInfo contains information about the parsed file
type FileInfo struct {
	PackageName   string    // Package name from package declaration
	Imports       []Import  // All import statements
	Targets       []*Target // Generation targets
	FilePath      string    // Source file path
	SourceContent string    // Full source file content
	SourceLines   []string  // Source content split by lines
}

// Import represents an import statement
type Import struct {
	Path  string // Import path (e.g., "context", "cloud.google.com/go/spanner")
	Alias string // Import alias (empty if no alias)
}

// Target represents a function or method to generate
type Target struct {
	Name           string         // Function or method name
	Receiver       *Receiver      // Receiver for methods (nil for functions)
	Params         []Param        // Function parameters
	Returns        []Return       // Return values
	Instruction    string         // Content from // mantra: comment
	FilePath       string         // Source file path
	HasPanic       bool           // Whether function contains panic("not implemented")
	Implementation string         // Generated implementation (temporary storage)
	FuncDecl       *ast.FuncDecl  // AST node for the function declaration
	TokenSet       *token.FileSet // Token file set for position information
}

// Receiver represents method receiver
type Receiver struct {
	Name string // Variable name (e.g., "r", "s")
	Type string // Type name (e.g., "*Repository", "Service")
}

// Param represents function parameter
type Param struct {
	Name string // Parameter name
	Type string // Parameter type
}

// Return represents return value
type Return struct {
	Type string // Return type
}

// ParseFileInfo parses a Go file and returns comprehensive file information
func ParseFileInfo(filePath string) (*FileInfo, error) {
	// Read source file content
	sourceContent, err := os.ReadFile(filePath)
	if err != nil {
		return nil, fmt.Errorf("failed to read file: %w", err)
	}

	fset := token.NewFileSet()
	node, err := parser.ParseFile(fset, filePath, sourceContent, parser.ParseComments)
	if err != nil {
		return nil, fmt.Errorf("failed to parse file: %w", err)
	}

	fileInfo := &FileInfo{
		PackageName:   node.Name.Name,
		FilePath:      filePath,
		SourceContent: string(sourceContent),
		SourceLines:   strings.Split(string(sourceContent), "\n"),
	}

	// Parse imports
	for _, imp := range node.Imports {
		importInfo := Import{
			Path: strings.Trim(imp.Path.Value, `"`),
		}
		if imp.Name != nil {
			importInfo.Alias = imp.Name.Name
		}
		fileInfo.Imports = append(fileInfo.Imports, importInfo)
	}

	// Parse targets using existing logic
	targets, err := parseTargetsFromNode(node, fset, filePath)
	if err != nil {
		return nil, err
	}
	fileInfo.Targets = targets

	return fileInfo, nil
}

// ParseFile parses a Go file and returns all generation targets (backwards compatibility)
func ParseFile(filePath string) ([]*Target, error) {
	fileInfo, err := ParseFileInfo(filePath)
	if err != nil {
		return nil, err
	}
	return fileInfo.Targets, nil
}

// parseTargetsFromNode extracts targets from parsed AST node
func parseTargetsFromNode(node *ast.File, fset *token.FileSet, filePath string) ([]*Target, error) {
	var targets []*Target

	// Map to store mantra comments by position
	mantraComments := make(map[token.Pos]string)

	// First pass: collect all // mantra: comments
	for _, commentGroup := range node.Comments {
		var mantraInstruction strings.Builder
		foundMantra := false

		for _, comment := range commentGroup.List {
			text := strings.TrimSpace(comment.Text)
			if strings.HasPrefix(text, "// mantra:") {
				foundMantra = true
				instruction := strings.TrimSpace(strings.TrimPrefix(text, "// mantra:"))
				mantraInstruction.WriteString(instruction)
			} else if foundMantra && strings.HasPrefix(text, "//") {
				// Continuation of mantra comment
				additionalText := strings.TrimSpace(strings.TrimPrefix(text, "//"))
				if additionalText != "" {
					mantraInstruction.WriteString("\n")
					mantraInstruction.WriteString(additionalText)
				}
			}
		}

		if foundMantra {
			// Store comment with its end position
			mantraComments[commentGroup.End()] = mantraInstruction.String()
		}
	}

	// Second pass: find functions with mantra comments
	ast.Inspect(node, func(n ast.Node) bool {
		switch x := n.(type) {
		case *ast.FuncDecl:
			// Check if there's a mantra comment immediately before this function
			var instruction string
			var found bool

			// Look for mantra comment right before function
			for pos, instr := range mantraComments {
				if pos < x.Pos() && x.Pos()-pos < maxCommentGap {
					instruction = instr
					found = true
					break
				}
			}

			if !found {
				return true
			}

			// Check if function contains panic("not implemented")
			hasPanic := containsNotImplementedPanic(x.Body)

			target := &Target{
				Name:        x.Name.Name,
				Instruction: instruction,
				FilePath:    filePath,
				HasPanic:    hasPanic,
				FuncDecl:    x,
				TokenSet:    fset,
			}

			// Parse receiver for methods
			if x.Recv != nil && len(x.Recv.List) > 0 {
				recv := x.Recv.List[0]
				target.Receiver = &Receiver{
					Type: astutils.GetTypeString(recv.Type),
				}
				if len(recv.Names) > 0 {
					target.Receiver.Name = recv.Names[0].Name
				}
			}

			// Parse parameters
			if x.Type.Params != nil {
				for _, field := range x.Type.Params.List {
					paramType := astutils.GetTypeString(field.Type)
					if len(field.Names) == 0 {
						// Unnamed parameter
						target.Params = append(target.Params, Param{
							Type: paramType,
						})
					} else {
						// Named parameters
						for _, name := range field.Names {
							target.Params = append(target.Params, Param{
								Name: name.Name,
								Type: paramType,
							})
						}
					}
				}
			}

			// Parse return values
			if x.Type.Results != nil {
				for _, field := range x.Type.Results.List {
					retType := astutils.GetTypeString(field.Type)
					// Return values can have multiple types in one field
					if len(field.Names) == 0 {
						target.Returns = append(target.Returns, Return{
							Type: retType,
						})
					} else {
						// Named returns (rare but possible)
						for range field.Names {
							target.Returns = append(target.Returns, Return{
								Type: retType,
							})
						}
					}
				}
			}

			targets = append(targets, target)
		}
		return true
	})

	return targets, nil
}

// containsNotImplementedPanic checks if function body contains panic("not implemented")
func containsNotImplementedPanic(body *ast.BlockStmt) bool {
	if body == nil {
		return false
	}

	found := false
	ast.Inspect(body, func(n ast.Node) bool {
		if callExpr, ok := n.(*ast.CallExpr); ok {
			if ident, ok := callExpr.Fun.(*ast.Ident); ok && ident.Name == "panic" {
				if len(callExpr.Args) == 1 {
					if lit, ok := callExpr.Args[0].(*ast.BasicLit); ok {
						if lit.Kind == token.STRING && lit.Value == `"not implemented"` {
							found = true
							return false
						}
					}
				}
			}
		}
		return true
	})

	return found
}

// GetFunctionSignature returns a string representation of the function signature
func (t *Target) GetFunctionSignature() string {
	var sig strings.Builder

	sig.WriteString("func ")

	// Add receiver if it's a method
	if t.Receiver != nil {
		sig.WriteString("(")
		if t.Receiver.Name != "" {
			sig.WriteString(t.Receiver.Name)
			sig.WriteString(" ")
		}
		sig.WriteString(t.Receiver.Type)
		sig.WriteString(") ")
	}

	sig.WriteString(t.Name)
	sig.WriteString("(")

	// Add parameters
	for i, param := range t.Params {
		if i > 0 {
			sig.WriteString(", ")
		}
		if param.Name != "" {
			sig.WriteString(param.Name)
			sig.WriteString(" ")
		}
		sig.WriteString(param.Type)
	}

	sig.WriteString(")")

	// Add return values
	if len(t.Returns) > 0 {
		sig.WriteString(" ")
		if len(t.Returns) > 1 {
			sig.WriteString("(")
		}
		for i, ret := range t.Returns {
			if i > 0 {
				sig.WriteString(", ")
			}
			sig.WriteString(ret.Type)
		}
		if len(t.Returns) > 1 {
			sig.WriteString(")")
		}
	}

	return sig.String()
}
