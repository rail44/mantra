package setup

import (
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/impl"
)

// InitializeRegistry creates and populates a tool registry with all available tools
func InitializeRegistry(projectRoot string) *tools.Registry {
	registry := tools.NewRegistry()

	// Register all tools
	registry.Register(impl.NewInspectTool())
	registry.Register(impl.NewSearchTool(projectRoot))
	registry.Register(impl.NewReadBodyTool(projectRoot))
	registry.Register(impl.NewCheckSyntaxTool())

	return registry
}

// DefaultExecutor creates a default tool executor with all tools registered
func DefaultExecutor(projectRoot string) *tools.Executor {
	registry := InitializeRegistry(projectRoot)
	return tools.NewExecutor(registry)
}