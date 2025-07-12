package modelfile

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/rail44/glyph/internal/parser"
)

// Renderer handles Modelfile generation
type Renderer struct {
	mode string
}

// NewRenderer creates a new Modelfile renderer
func NewRenderer(mode string) *Renderer {
	if mode == "" {
		mode = "generic"
	}
	return &Renderer{mode: mode}
}

// GenerateModelfile creates a Modelfile for the given context
func (r *Renderer) GenerateModelfile(decl *parser.Declaration) (string, error) {
	// Load mode configuration
	config, err := LoadModeConfig(r.mode)
	if err != nil {
		return "", fmt.Errorf("failed to load mode config: %w", err)
	}

	// Load system prompt
	systemPrompt, err := LoadSystemPrompt(r.mode)
	if err != nil {
		return "", fmt.Errorf("failed to load system prompt: %w", err)
	}

	// Select relevant examples based on the declaration
	examples := r.selectRelevantExamples(decl, config)

	// Build template data
	data := TemplateData{
		BaseModel:    config.BaseModel,
		SystemPrompt: r.buildFullSystemPrompt(systemPrompt, config),
		Examples:     examples,
		Parameters:   config.Parameters,
	}

	// Render the Modelfile
	return RenderModelfile(data)
}

// CreateTempModelfile writes a Modelfile to a temporary location
func (r *Renderer) CreateTempModelfile(decl *parser.Declaration) (string, error) {
	content, err := r.GenerateModelfile(decl)
	if err != nil {
		return "", err
	}

	// Create temp file
	tmpDir := os.TempDir()
	tmpFile := filepath.Join(tmpDir, fmt.Sprintf("glyph_%s_%d.modelfile", r.mode, os.Getpid()))
	
	if err := os.WriteFile(tmpFile, []byte(content), 0644); err != nil {
		return "", fmt.Errorf("failed to write temp modelfile: %w", err)
	}

	return tmpFile, nil
}

func (r *Renderer) buildFullSystemPrompt(base string, config *ModeConfig) string {
	prompt := base + "\n\n"

	if len(config.Principles) > 0 {
		prompt += "Key principles:\n"
		for _, principle := range config.Principles {
			prompt += fmt.Sprintf("- %s\n", principle)
		}
		prompt += "\n"
	}

	return prompt
}

func (r *Renderer) selectRelevantExamples(decl *parser.Declaration, config *ModeConfig) []Example {
	var examples []Example

	// For now, convert patterns to examples
	// In the future, this could use similarity matching
	for _, pattern := range config.Patterns {
		// Simple heuristic: include CRUD patterns for Request types
		if r.isRelevantPattern(decl, pattern) {
			examples = append(examples, Example{
				Input:  fmt.Sprintf("// %s\n// %s", pattern.Name, pattern.Description),
				Output: pattern.Example,
			})
		}
	}

	return examples
}

func (r *Renderer) isRelevantPattern(decl *parser.Declaration, pattern Pattern) bool {
	// Simple relevance check - can be made more sophisticated
	switch pattern.Name {
	case "Query Pattern":
		return true // Always relevant for now
	case "Single Row Read":
		// Check if it looks like a single item request
		if len(decl.Fields) == 1 {
			fieldName := decl.Fields[0].Name
			return strings.HasSuffix(fieldName, "ID") || fieldName == "Id"
		}
		return false
	default:
		return false
	}
}