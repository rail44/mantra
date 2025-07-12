package generator

import (
	"fmt"
	"go/format"
	"os"
	"path/filepath"
	"strings"
)

type Generator struct {
	outputPath string
}

func New(declarationPath string) *Generator {
	return &Generator{
		outputPath: getOutputPath(declarationPath),
	}
}

// Generate processes the AI response and creates the implementation file
func (g *Generator) Generate(aiResponse string) error {
	// Clean the response (remove any markdown formatting if present)
	code := cleanCode(aiResponse)

	// Format the Go code
	formatted, err := format.Source([]byte(code))
	if err != nil {
		// If formatting fails, use the original code but log the error
		fmt.Fprintf(os.Stderr, "Warning: failed to format generated code: %v\n", err)
		formatted = []byte(code)
	}

	// Ensure directory exists
	dir := filepath.Dir(g.outputPath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create directory: %w", err)
	}

	// Write the file
	if err := os.WriteFile(g.outputPath, formatted, 0644); err != nil {
		return fmt.Errorf("failed to write implementation file: %w", err)
	}

	return nil
}

// GetOutputPath returns the path where the implementation will be written
func (g *Generator) GetOutputPath() string {
	return g.outputPath
}

func getOutputPath(declarationPath string) string {
	dir := filepath.Dir(declarationPath)
	base := filepath.Base(declarationPath)
	ext := filepath.Ext(base)
	name := strings.TrimSuffix(base, ext)
	return filepath.Join(dir, name+"_impl"+ext)
}

func cleanCode(response string) string {
	// Remove common markdown code block markers
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