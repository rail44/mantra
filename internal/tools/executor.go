package tools

import (
	"context"
	"fmt"
	"time"

	"github.com/rail44/mantra/internal/log"
	"log/slog"
)

// Executor handles tool execution with context and logging
type Executor struct {
	registry *Registry
	timeout  time.Duration
}

// NewExecutor creates a new tool executor
func NewExecutor(registry *Registry) *Executor {
	return &Executor{
		registry: registry,
		timeout:  30 * time.Second, // Default timeout
	}
}

// Execute runs a tool by name with the given parameters
func (e *Executor) Execute(ctx context.Context, toolName string, params map[string]interface{}) (interface{}, error) {
	// Get the tool from registry
	tool, err := e.registry.Get(toolName)
	if err != nil {
		return nil, &ToolError{
			Code:    "tool_not_found",
			Message: fmt.Sprintf("Tool %q not found", toolName),
		}
	}

	// Create a context with timeout
	execCtx, cancel := context.WithTimeout(ctx, e.timeout)
	defer cancel()

	// Log the execution
	log.Debug("executing tool",
		slog.String("tool", toolName),
		slog.Any("params", params))

	// Execute the tool
	start := time.Now()
	result, err := tool.Execute(execCtx, params)
	duration := time.Since(start)

	// Log the result
	if err != nil {
		log.Error("tool execution failed",
			slog.String("tool", toolName),
			slog.Duration("duration", duration),
			slog.String("error", err.Error()))
		return nil, err
	}

	log.Debug("tool execution completed",
		slog.String("tool", toolName),
		slog.Duration("duration", duration))

	return result, nil
}
