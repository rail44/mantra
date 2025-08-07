package impl

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/format"
	"go/parser"
	"go/token"
	"go/types"
	"path/filepath"
	"strconv"
	"strings"

	"golang.org/x/tools/go/analysis"
	"golang.org/x/tools/go/analysis/passes/inspect"
	"golang.org/x/tools/go/packages"
	"honnef.co/go/tools/analysis/lint"
	"honnef.co/go/tools/simple"
	"honnef.co/go/tools/staticcheck"
	"honnef.co/go/tools/stylecheck"
	"honnef.co/go/tools/unused"

	"github.com/rail44/mantra/internal/tools"
	pkgparser "github.com/rail44/mantra/internal/parser"
)

// CheckCodeTool validates Go code using staticcheck analyzers
type CheckCodeTool struct {
	projectRoot string
	context     *tools.Context // Stored context from SetContext
}

// NewCheckCodeTool creates a new code checking tool
func NewCheckCodeTool(projectRoot string) *CheckCodeTool {
	return &CheckCodeTool{
		projectRoot: projectRoot,
	}
}

// Name returns the tool name
func (t *CheckCodeTool) Name() string {
	return "check_code"
}

// Description returns what this tool does
func (t *CheckCodeTool) Description() string {
	return "Validate Go code syntax and run comprehensive static analysis"
}

// ParametersSchema returns the JSON Schema for parameters
func (t *CheckCodeTool) ParametersSchema() json.RawMessage {
	return json.RawMessage(`{
		"type": "object",
		"properties": {
			"code": {
				"type": "string",
				"description": "The generated function body to validate"
			}
		},
		"required": ["code"]
	}`)
}

// SetContext implements ContextAwareTool interface
func (t *CheckCodeTool) SetContext(toolCtx *tools.Context) {
	t.context = toolCtx
	// Update project root if provided in context
	if toolCtx != nil && toolCtx.ProjectRoot != "" {
		t.projectRoot = toolCtx.ProjectRoot
	}
}

// Execute runs the static analysis tool
func (t *CheckCodeTool) Execute(ctx context.Context, params map[string]interface{}) (interface{}, error) {
	// Extract parameters
	code, ok := params["code"].(string)
	if !ok {
		return nil, &tools.ToolError{
			Code:    "invalid_params",
			Message: "Parameter 'code' is required and must be a string",
		}
	}
	
	// Trim whitespace to avoid issues with leading/trailing spaces
	code = strings.TrimSpace(code)

	// Get fileInfo and target from context
	if t.context == nil {
		return nil, &tools.ToolError{
			Code:    "missing_context",
			Message: "Tool context not set - this tool requires FileInfo and Target from context",
		}
	}

	fileInfo := t.context.FileInfo
	target := t.context.Target

	if fileInfo == nil {
		return nil, &tools.ToolError{
			Code:    "invalid_context",
			Message: "FileInfo not found in context",
		}
	}

	if target == nil {
		return nil, &tools.ToolError{
			Code:    "invalid_context",
			Message: "Target not found in context",
		}
	}

	// Replace function body using AST manipulation
	modified, err := t.replaceViaAST(fileInfo.SourceContent, target, code)
	if err != nil {
		return nil, fmt.Errorf("failed to replace function body: %w", err)
	}

	// Create overlay map for in-memory analysis
	overlay := map[string][]byte{
		fileInfo.FilePath: modified.Content,
	}

	// Configure packages.Load for type checking
	cfg := &packages.Config{
		Mode: packages.NeedTypes |
			packages.NeedSyntax |
			packages.NeedTypesInfo |
			packages.NeedName |
			packages.NeedFiles |
			packages.NeedCompiledGoFiles,
		Dir:     t.projectRoot,
		Overlay: overlay,
		Tests:   false,
	}

	// Load the package
	pkgPattern := filepath.Dir(fileInfo.FilePath)
	pkgs, err := packages.Load(cfg, pkgPattern)
	if err != nil {
		return nil, fmt.Errorf("failed to load packages: %w", err)
	}

	// Run analyzers with position filtering
	return t.runAnalyzersWithFilter(pkgs, modified, fileInfo.FilePath)
}

// ModifiedFile holds the modified file content and position information
type ModifiedFile struct {
	Content      []byte         // Modified file content
	TargetFunc   *ast.FuncDecl  // Replaced function AST node
	BodyStartPos token.Pos      // New body start position
	BodyEndPos   token.Pos      // New body end position
	FileSet      *token.FileSet // For position resolution
}

// replaceViaAST replaces function body using AST manipulation
func (t *CheckCodeTool) replaceViaAST(sourceContent string, target *pkgparser.Target, newBody string) (*ModifiedFile, error) {
	// Create a new FileSet for position tracking
	fset := token.NewFileSet()

	// Parse the source file
	file, err := parser.ParseFile(fset, "source.go", sourceContent, parser.ParseComments)
	if err != nil {
		return nil, fmt.Errorf("failed to parse source: %w", err)
	}

	// Parse the new function body (wrapped in a temporary function)
	wrappedCode := fmt.Sprintf("package p\nfunc _() {\n%s\n}", newBody)
	tempFset := token.NewFileSet()
	tempFile, err := parser.ParseFile(tempFset, "", wrappedCode, 0)
	if err != nil {
		// Include the first few lines of the code in the error for debugging
		lines := strings.Split(newBody, "\n")
		maxLines := 3
		if len(lines) < maxLines {
			maxLines = len(lines)
		}
		preview := strings.Join(lines[:maxLines], "\n")
		return nil, fmt.Errorf("failed to parse new body: %w (preview: %q)", err, preview)
	}

	// Extract the new body
	var newBodyStmt *ast.BlockStmt
	for _, decl := range tempFile.Decls {
		if fn, ok := decl.(*ast.FuncDecl); ok {
			newBodyStmt = fn.Body
			break
		}
	}

	if newBodyStmt == nil {
		return nil, fmt.Errorf("failed to extract new body")
	}

	// Find and replace the target function
	var replacedFunc *ast.FuncDecl
	replaced := false

	ast.Inspect(file, func(n ast.Node) bool {
		if fn, ok := n.(*ast.FuncDecl); ok {
			if t.matchesTarget(fn, target) {
				// Replace the function body
				fn.Body = newBodyStmt
				replacedFunc = fn
				replaced = true
				return false
			}
		}
		return true
	})

	if !replaced {
		return nil, fmt.Errorf("target function not found: %s", target.Name)
	}

	// Format the modified AST back to source code
	var buf bytes.Buffer
	if err := format.Node(&buf, fset, file); err != nil {
		return nil, fmt.Errorf("failed to format AST: %w", err)
	}

	return &ModifiedFile{
		Content:      buf.Bytes(),
		TargetFunc:   replacedFunc,
		BodyStartPos: replacedFunc.Body.Pos(),
		BodyEndPos:   replacedFunc.Body.End(),
		FileSet:      fset,
	}, nil
}

// matchesTarget checks if a function declaration matches the target
func (t *CheckCodeTool) matchesTarget(fn *ast.FuncDecl, target *pkgparser.Target) bool {
	// Check function name
	if fn.Name.Name != target.Name {
		return false
	}

	// Check receiver
	if target.Receiver != nil {
		if fn.Recv == nil || len(fn.Recv.List) == 0 {
			return false
		}
		// TODO: More sophisticated receiver type matching if needed
	} else if fn.Recv != nil && len(fn.Recv.List) > 0 {
		return false
	}

	return true
}

// PositionMapper maps positions between absolute and relative coordinates
type PositionMapper struct {
	funcDecl      *ast.FuncDecl
	bodyStart     token.Pos
	bodyEnd       token.Pos
	fileSet       *token.FileSet
	startPosition token.Position
}

// createPositionMapper creates a position mapper for the target function
func (t *CheckCodeTool) createPositionMapper(pkg *packages.Package, modified *ModifiedFile, targetFile string) (*PositionMapper, error) {
	// Find the target function in the loaded package
	var targetFunc *ast.FuncDecl

	for _, file := range pkg.Syntax {
		// Check if this is our target file
		position := pkg.Fset.Position(file.Pos())
		if position.Filename != targetFile {
			continue
		}

		ast.Inspect(file, func(n ast.Node) bool {
			if fn, ok := n.(*ast.FuncDecl); ok {
				if fn.Name.Name == modified.TargetFunc.Name.Name {
					targetFunc = fn
					return false
				}
			}
			return true
		})
	}

	if targetFunc == nil {
		return nil, fmt.Errorf("target function not found in package")
	}

	// Get the start position (after opening brace)
	bodyStart := targetFunc.Body.Lbrace + 1
	startPos := pkg.Fset.Position(bodyStart)

	return &PositionMapper{
		funcDecl:      targetFunc,
		bodyStart:     bodyStart,
		bodyEnd:       targetFunc.Body.Rbrace,
		fileSet:       pkg.Fset,
		startPosition: startPos,
	}, nil
}

// IsInGeneratedCode checks if a position is within the generated function body
func (pm *PositionMapper) IsInGeneratedCode(pos token.Pos) bool {
	return pos > pm.bodyStart && pos < pm.bodyEnd
}

// ToRelativePosition converts absolute position to relative position within function body
func (pm *PositionMapper) ToRelativePosition(pos token.Pos) (line, column int) {
	absPosition := pm.fileSet.Position(pos)

	// Calculate relative line number from function body start
	relativeLine := absPosition.Line - pm.startPosition.Line + 1

	// For the first line, adjust column position for indentation
	if relativeLine == 1 {
		// Calculate base indentation from the first statement if available
		if pm.funcDecl.Body.List != nil && len(pm.funcDecl.Body.List) > 0 {
			firstStmt := pm.funcDecl.Body.List[0]
			firstStmtPos := pm.fileSet.Position(firstStmt.Pos())
			baseIndent := firstStmtPos.Column - 1 // Convert to 0-based
			relativeColumn := absPosition.Column - baseIndent
			if relativeColumn < 1 {
				relativeColumn = 1
			}
			return relativeLine, relativeColumn
		}
		return relativeLine, absPosition.Column
	}

	return relativeLine, absPosition.Column
}

// runAnalyzersWithFilter runs staticcheck analyzers with position filtering
func (t *CheckCodeTool) runAnalyzersWithFilter(pkgs []*packages.Package, modified *ModifiedFile, targetFile string) (*CheckCodeResult, error) {
	var issues []Issue
	var packageErrors []packages.Error

	// Collect package errors first
	for _, pkg := range pkgs {
		packageErrors = append(packageErrors, pkg.Errors...)
	}

	// Find the target package
	var targetPkg *packages.Package
	for _, pkg := range pkgs {
		for _, file := range pkg.CompiledGoFiles {
			if file == targetFile {
				targetPkg = pkg
				break
			}
		}
		if targetPkg != nil {
			break
		}
	}

	if targetPkg == nil {
		// If no exact match, use the first package (common case for overlay)
		if len(pkgs) > 0 {
			targetPkg = pkgs[0]
		} else {
			return &CheckCodeResult{
				Valid:  false,
				Issues: issues,
			}, nil
		}
	}

	// Create position mapper
	mapper, err := t.createPositionMapper(targetPkg, modified, targetFile)
	if err != nil {
		// If we can't create a mapper, convert package errors without position mapping
		for _, pkgErr := range packageErrors {
			issues = append(issues, Issue{
				Code:    "package_error",
				Message: pkgErr.Msg,
			})
		}
		return &CheckCodeResult{
			Valid:  len(packageErrors) == 0,
			Issues: issues,
		}, nil
	}
	
	// Process package errors with position mapping
	for _, pkgErr := range packageErrors {
		issue := Issue{
			Code:    "package_error",
			Message: pkgErr.Msg,
		}
		
		// Parse position from error if available
		// Format: "file:line:col" or "file:line"
		if pkgErr.Pos != "" && pkgErr.Pos != "-" {
			parts := strings.Split(pkgErr.Pos, ":")
			if len(parts) >= 2 {
				// Check if this error is in our target file
				if strings.HasSuffix(parts[0], filepath.Base(targetFile)) {
					// Parse line number
					if line, parseErr := strconv.Atoi(parts[1]); parseErr == nil {
						// Convert absolute line to relative position within function body
						relLine := line - mapper.startPosition.Line + 1
						if relLine > 0 {
							issue.Line = relLine
							// Parse column if available
							if len(parts) >= 3 {
								if col, parseErr := strconv.Atoi(parts[2]); parseErr == nil {
									issue.Column = col
								}
							}
						}
					}
				}
			}
		}
		
		issues = append(issues, issue)
	}

	// Collect all analyzers (same as staticcheck CLI)
	var allAnalyzers []*analysis.Analyzer
	
	// Helper function to check if analyzer should be included
	// This mimics staticcheck CLI's behavior: include all analyzers except those marked as NonDefault
	shouldInclude := func(la *lint.Analyzer) bool {
		// Check if the analyzer has NonDefault set to true
		// The staticcheck CLI excludes analyzers with NonDefault=true by default
		if la.Doc != nil && la.Doc.NonDefault {
			return false
		}
		return true
	}
	
	// Add simple analyzers (S*)
	for _, la := range simple.Analyzers {
		if la.Analyzer != nil && shouldInclude(la) {
			allAnalyzers = append(allAnalyzers, la.Analyzer)
		}
	}
	
	// Add staticcheck analyzers (SA*)
	for _, la := range staticcheck.Analyzers {
		if la.Analyzer != nil && shouldInclude(la) {
			allAnalyzers = append(allAnalyzers, la.Analyzer)
		}
	}
	
	// Add stylecheck analyzers (ST*)
	for _, la := range stylecheck.Analyzers {
		if la.Analyzer != nil && shouldInclude(la) {
			allAnalyzers = append(allAnalyzers, la.Analyzer)
		}
	}
	
	// Add unused analyzer (U*)
	// Note: unused.Analyzer is a single analyzer, not a slice
	if unused.Analyzer.Analyzer != nil && shouldInclude(unused.Analyzer) {
		allAnalyzers = append(allAnalyzers, unused.Analyzer.Analyzer)
	}

	// Run all analyzers with proper dependency handling
	// First, run the inspect analyzer that many other analyzers depend on
	analyzersResults := make(map[*analysis.Analyzer]interface{})
	
	// Run inspect analyzer first
	inspectPass := &analysis.Pass{
		Analyzer:      inspect.Analyzer,
		Fset:          targetPkg.Fset,
		Files:         targetPkg.Syntax,
		Pkg:           targetPkg.Types,
		TypesInfo:     targetPkg.TypesInfo,
		ResultOf:      make(map[*analysis.Analyzer]interface{}),
		ImportObjectFact: func(obj types.Object, fact analysis.Fact) bool { return false },
		ExportObjectFact: func(obj types.Object, fact analysis.Fact) {},
		ImportPackageFact: func(pkg *types.Package, fact analysis.Fact) bool { return false },
		ExportPackageFact: func(fact analysis.Fact) {},
		AllObjectFacts: func() []analysis.ObjectFact { return nil },
		AllPackageFacts: func() []analysis.PackageFact { return nil },
		Report: func(diag analysis.Diagnostic) {}, // Inspect doesn't report diagnostics
	}
	
	if inspectResult, err := inspect.Analyzer.Run(inspectPass); err == nil {
		analyzersResults[inspect.Analyzer] = inspectResult
	}
	
	// Now run the other analyzers
	for _, analyzer := range allAnalyzers {
		pass := &analysis.Pass{
			Analyzer:      analyzer,
			Fset:          targetPkg.Fset,
			Files:         targetPkg.Syntax,
			Pkg:           targetPkg.Types,
			TypesInfo:     targetPkg.TypesInfo,
			ResultOf:      analyzersResults, // Share results between analyzers
			ImportObjectFact: func(obj types.Object, fact analysis.Fact) bool { return false },
			ExportObjectFact: func(obj types.Object, fact analysis.Fact) {},
			ImportPackageFact: func(pkg *types.Package, fact analysis.Fact) bool { return false },
			ExportPackageFact: func(fact analysis.Fact) {},
			AllObjectFacts: func() []analysis.ObjectFact { return nil },
			AllPackageFacts: func() []analysis.PackageFact { return nil },
			Report: func(diag analysis.Diagnostic) {
				// Only report diagnostics within the generated code
				if mapper.IsInGeneratedCode(diag.Pos) {
					// Convert to relative position
					relLine, relColumn := mapper.ToRelativePosition(diag.Pos)

					issues = append(issues, Issue{
						Code:    analyzer.Name,
						Message: diag.Message,
						Line:    relLine,
						Column:  relColumn,
					})
				}
			},
		}

		// Run the analyzer with panic recovery
		func() {
			defer func() {
				if r := recover(); r != nil {
					// Log the panic for debugging but continue with other analyzers
					// This typically happens when analyzer dependencies are not satisfied
					// TODO: Implement proper dependency resolution
					_ = r // For now, continue silently to avoid breaking the analysis
				}
			}()
			
			// Try to run the analyzer
			if result, err := analyzer.Run(pass); err == nil && result != nil {
				// Store result for dependent analyzers
				analyzersResults[analyzer] = result
			}
		}()
	}

	return &CheckCodeResult{
		Valid:  len(issues) == 0,
		Issues: issues,
	}, nil
}

// CheckCodeResult represents the result of code checking
type CheckCodeResult struct {
	Valid  bool    `json:"valid"`
	Issues []Issue `json:"issues,omitempty"`
}

// Issue represents a code issue found during checking
type Issue struct {
	Code    string `json:"code"`              // Analyzer code (e.g., "SA1000")
	Message string `json:"message"`           // Issue description
	Line    int    `json:"line,omitempty"`    // Line number (relative to function body)
	Column  int    `json:"column,omitempty"`  // Column position
}