package tools

import (
	"fmt"
	"sync"
)

// Registry manages available tools
type Registry struct {
	mu    sync.RWMutex
	tools map[string]Tool
}

// NewRegistry creates a new tool registry
func NewRegistry() *Registry {
	return &Registry{
		tools: make(map[string]Tool),
	}
}

// Register adds a tool to the registry
func (r *Registry) Register(tool Tool) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	name := tool.Name()
	if _, exists := r.tools[name]; exists {
		return fmt.Errorf("tool %q already registered", name)
	}
	
	r.tools[name] = tool
	return nil
}

// Get retrieves a tool by name
func (r *Registry) Get(name string) (Tool, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	
	tool, exists := r.tools[name]
	if !exists {
		return nil, fmt.Errorf("tool %q not found", name)
	}
	
	return tool, nil
}

// ListAvailable returns all available tool definitions
func (r *Registry) ListAvailable() []ToolDefinition {
	r.mu.RLock()
	defer r.mu.RUnlock()
	
	definitions := make([]ToolDefinition, 0, len(r.tools))
	for _, tool := range r.tools {
		definitions = append(definitions, ToolDefinition{
			Name:        tool.Name(),
			Description: tool.Description(),
			Parameters:  tool.ParametersSchema(),
		})
	}
	
	return definitions
}