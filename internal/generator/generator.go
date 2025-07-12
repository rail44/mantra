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
	functionCode := cleanCode(aiResponse)

	// Read the existing implementation file if it exists
	existingContent := ""
	if data, err := os.ReadFile(g.outputPath); err == nil {
		existingContent = string(data)
	}

	// Build the complete file content
	var fullCode string
	if existingContent != "" {
		// If file exists, append the new function
		fullCode = existingContent + "\n" + functionCode
	} else {
		// If file doesn't exist, create a new one with imports
		fullCode = g.buildNewFile(functionCode)
	}

	// Format the Go code
	formatted, err := format.Source([]byte(fullCode))
	if err != nil {
		// If formatting fails, use the original code but log the error
		fmt.Fprintf(os.Stderr, "Warning: failed to format generated code: %v\n", err)
		formatted = []byte(fullCode)
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

func (g *Generator) buildNewFile(functionCode string) string {
	// Extract package name from the output path
	dir := filepath.Base(filepath.Dir(g.outputPath))
	if dir == "." || dir == "/" {
		dir = "main"
	}

	// Build a new file with standard imports
	var sb strings.Builder
	sb.WriteString(fmt.Sprintf("package %s\n\n", dir))
	sb.WriteString("import (\n")
	sb.WriteString("\t\"context\"\n")
	sb.WriteString("\t\"fmt\"\n")
	sb.WriteString("\t\"time\"\n")
	sb.WriteString("\n")
	sb.WriteString("\t\"cloud.google.com/go/spanner\"\n")
	sb.WriteString("\t\"google.golang.org/api/iterator\"\n")
	sb.WriteString(")\n\n")
	sb.WriteString(functionCode)
	
	return sb.String()
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