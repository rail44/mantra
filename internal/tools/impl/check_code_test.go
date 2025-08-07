package impl

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rail44/mantra/internal/parser"
	"github.com/rail44/mantra/internal/tools"
)

func TestCheckCodeTool_DefaultConfiguration(t *testing.T) {
	// Create a temporary directory for the test
	tmpDir := t.TempDir()
	testFile := filepath.Join(tmpDir, "test.go")
	
	// Create test code that would trigger ST1000 (missing package comment)
	// This should NOT be reported as ST1000 is disabled by default
	testCode := `
	return fmt.Sprintf("Hello, %s!", name)
	`

	// Parse a minimal test file
	testFileContent := `package test

import "fmt"

func Greet(name string) string {
	panic("not implemented")
}
`

	// Write the test file
	if err := os.WriteFile(testFile, []byte(testFileContent), 0644); err != nil {
		t.Fatalf("Failed to write test file: %v", err)
	}
	
	// Create a go.mod file for the test package
	goModContent := `module test

go 1.21
`
	goModFile := filepath.Join(tmpDir, "go.mod")
	if err := os.WriteFile(goModFile, []byte(goModContent), 0644); err != nil {
		t.Fatalf("Failed to write go.mod file: %v", err)
	}

	fileInfo := &parser.FileInfo{
		FilePath:      testFile,
		PackageName:   "test",
		SourceContent: testFileContent,
		Imports: []parser.Import{
			{Path: "fmt"},
		},
	}

	target := &parser.Target{
		Name:        "Greet",
		FilePath:    testFile,
		Instruction: "Greet the user",
		Params: []parser.Param{
			{Name: "name", Type: "string"},
		},
		Returns: []parser.Return{
			{Type: "string"},
		},
	}

	// Create tool and context
	tool := NewCheckCodeTool(tmpDir)
	toolContext := tools.NewContext(fileInfo, target, tmpDir)
	tool.SetContext(toolContext)

	// Execute the tool
	ctx := context.Background()
	params := map[string]interface{}{
		"code": testCode,
	}

	result, err := tool.Execute(ctx, params)
	if err != nil {
		t.Fatalf("Failed to execute tool: %v", err)
	}
	
	t.Logf("Result: %+v", result)

	checkResult, ok := result.(*CheckCodeResult)
	if !ok {
		t.Fatalf("Result is not *CheckCodeResult")
	}

	// The code should be valid (no syntax errors)
	if !checkResult.Valid {
		t.Logf("Package errors or issues found")
		t.Errorf("Expected code to be valid, but got invalid. Issues: %+v", checkResult.Issues)
	}

	// Check that ST1000 is NOT reported (it's disabled by default)
	for _, issue := range checkResult.Issues {
		if issue.Code == "ST1000" {
			t.Errorf("ST1000 should not be reported (disabled by default), but was found")
		}
		if issue.Code == "ST1003" {
			t.Errorf("ST1003 should not be reported (disabled by default), but was found")
		}
	}
}

func TestCheckCodeTool_FindsActualIssues(t *testing.T) {
	// Create a temporary directory for the test
	tmpDir := t.TempDir()
	testFile := filepath.Join(tmpDir, "test.go")
	
	// Create test code with an actual issue that should be caught
	// Using a variable that's never used should trigger SA4006
	testCode := `
	unusedVar := "this is never used"
	return fmt.Sprintf("Hello, %s!", name)
	`

	// Parse a minimal test file
	testFileContent := `package test

import "fmt"

func Greet(name string) string {
	panic("not implemented")
}
`

	// Write the test file
	if err := os.WriteFile(testFile, []byte(testFileContent), 0644); err != nil {
		t.Fatalf("Failed to write test file: %v", err)
	}
	
	// Create a go.mod file for the test package
	goModContent := `module test

go 1.21
`
	goModFile := filepath.Join(tmpDir, "go.mod")
	if err := os.WriteFile(goModFile, []byte(goModContent), 0644); err != nil {
		t.Fatalf("Failed to write go.mod file: %v", err)
	}

	fileInfo := &parser.FileInfo{
		FilePath:      testFile,
		PackageName:   "test",
		SourceContent: testFileContent,
		Imports: []parser.Import{
			{Path: "fmt"},
		},
	}

	target := &parser.Target{
		Name:        "Greet",
		FilePath:    testFile,
		Instruction: "Greet the user",
		Params: []parser.Param{
			{Name: "name", Type: "string"},
		},
		Returns: []parser.Return{
			{Type: "string"},
		},
	}

	// Create tool and context
	tool := NewCheckCodeTool(tmpDir)
	toolContext := tools.NewContext(fileInfo, target, tmpDir)
	tool.SetContext(toolContext)

	// Execute the tool
	ctx := context.Background()
	params := map[string]interface{}{
		"code": testCode,
	}

	result, err := tool.Execute(ctx, params)
	if err != nil {
		t.Fatalf("Failed to execute tool: %v", err)
	}

	checkResult, ok := result.(*CheckCodeResult)
	if !ok {
		t.Fatalf("Result is not *CheckCodeResult")
	}

	// The code should have issues
	if checkResult.Valid {
		t.Errorf("Expected code to have issues, but was marked as valid")
	}

	// Should find the unused variable issue
	foundUnusedVar := false
	for _, issue := range checkResult.Issues {
		if strings.Contains(issue.Message, "unusedVar") || 
		   strings.Contains(issue.Message, "never used") ||
		   issue.Code == "SA4006" {
			foundUnusedVar = true
			break
		}
	}

	if !foundUnusedVar {
		t.Errorf("Expected to find unused variable issue, but didn't. Issues: %+v", checkResult.Issues)
	}
}