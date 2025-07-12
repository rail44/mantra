package modelfile

import (
	"fmt"
)

// ModeConfig represents configuration for a generation mode
type ModeConfig struct {
	Name        string                 `yaml:"name"`
	Description string                 `yaml:"description"`
	BaseModel   string                 `yaml:"base_model"`
	Parameters  map[string]interface{} `yaml:"parameters"`
	Principles  []string               `yaml:"principles"`
	Patterns    []Pattern              `yaml:"patterns"`
}

// Pattern represents a code pattern example
type Pattern struct {
	Name        string `yaml:"name"`
	Description string `yaml:"description"`
	Example     string `yaml:"example"`
}

// LoadModeConfig loads configuration for a specific mode
func LoadModeConfig(modeName string) (*ModeConfig, error) {
	// Load from built-in modes only
	builtinConfig, exists := builtinModes[modeName]
	if !exists {
		return nil, fmt.Errorf("mode '%s' not found", modeName)
	}

	return &builtinConfig, nil
}


// LoadSystemPrompt loads the system prompt for a mode
func LoadSystemPrompt(modeName string) (string, error) {
	// Load from built-in prompts only
	prompt, exists := builtinSystemPrompts[modeName]
	if !exists {
		return "", fmt.Errorf("system prompt for mode '%s' not found", modeName)
	}

	return prompt, nil
}