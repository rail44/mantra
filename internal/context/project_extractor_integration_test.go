package context_test

import (
	"path/filepath"
	"testing"

	"github.com/rail44/mantra/internal/context"
	"github.com/rail44/mantra/internal/parser"
)

func TestExtractProjectContext_Integration(t *testing.T) {
	// Test with actual test data files
	testFile := filepath.Join("testdata", "standalone", "main.go")
	
	fileInfo, err := parser.ParseFileInfo(testFile)
	if err != nil {
		t.Fatalf("failed to parse file: %v", err)
	}
	
	if len(fileInfo.Targets) == 0 {
		t.Fatal("no targets found in test file")
	}
	
	target := fileInfo.Targets[0]
	ctx, err := context.ExtractProjectContext(testFile, target)
	if err != nil {
		t.Fatalf("ExtractProjectContext failed: %v", err)
	}
	
	// Verify results
	if ctx.PackageName != "main" {
		t.Errorf("PackageName = %v, want main", ctx.PackageName)
	}
	
	if _, exists := ctx.Types["Config"]; !exists {
		t.Error("Config type not found in extracted context")
	}
	
	t.Logf("Successfully extracted context from standalone file")
	t.Logf("Package: %s, Types: %d", ctx.PackageName, len(ctx.Types))
}