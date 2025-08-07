package context

import (
	"fmt"
	"path/filepath"

	"github.com/rail44/mantra/internal/analysis"
	"github.com/rail44/mantra/internal/parser"
)

// RelevantContext holds context information relevant to a function
type RelevantContext struct {
	Imports     []*ImportInfo                    // All imports with structured information
	Types       map[string]string                // Type definitions (name -> definition)
	Methods     map[string][]analysis.MethodInfo // Type methods (typeName -> methods)
	PackageName string                           // Package name
}

// ExtractFunctionContext extracts context using go/packages for accurate type resolution
func ExtractFunctionContext(filePath string, target *parser.Target) (*RelevantContext, error) {
	// Create package loader for the directory containing the file
	packagePath := filepath.Dir(filePath)
	loader := NewPackageLoader(packagePath)

	// Identify types directly referenced in function signature
	directlyUsedTypes := extractDirectlyUsedTypes(target)

	// Get context using the package loader
	// Pass the target method name to exclude it from the methods list
	targetMethodName := ""
	if target.Receiver != nil {
		targetMethodName = target.Name
	}
	ctx, err := loader.GetContextForTarget(filePath, directlyUsedTypes, targetMethodName)
	if err != nil {
		return nil, fmt.Errorf("failed to extract context: %w", err)
	}

	return ctx, nil
}

// extractDirectlyUsedTypes extracts types directly used in function signature
func extractDirectlyUsedTypes(target *parser.Target) map[string]bool {
	types := make(map[string]bool)

	// Add receiver type
	if target.Receiver != nil {
		typeName := analysis.CleanTypeName(target.Receiver.Type)
		if !analysis.IsBuiltinType(typeName) {
			types[typeName] = true
		}
	}

	// Add parameter types
	for _, param := range target.Params {
		typeName := analysis.CleanTypeName(param.Type)
		if !analysis.IsBuiltinType(typeName) {
			types[typeName] = true
		}
	}

	// Add return types
	for _, ret := range target.Returns {
		typeName := analysis.CleanTypeName(ret.Type)
		if !analysis.IsBuiltinType(typeName) {
			types[typeName] = true
		}
	}

	return types
}
