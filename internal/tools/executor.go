package tools

import (
	"context"
	"fmt"
	"time"

	"log/slog"

	"github.com/rail44/mantra/internal/log"
)

// Executor handles tool execution with context and logging
type Executor struct {
	tools   map[string]Tool
	timeout time.Duration
}

// NewExecutor creates a new tool executor
func NewExecutor(tools []Tool) *Executor {
	toolMap := make(map[string]Tool)
	for _, tool := range tools {
		toolMap[tool.Name()] = tool
	}
	return &Executor{
		tools:   toolMap,
		timeout: 30 * time.Second, // Default timeout
	}
}

// Execute runs a tool by name with the given parameters
func (e *Executor) Execute(ctx context.Context, toolName string, params map[string]interface{}) (interface{}, error) {
	// Get the tool from map
	tool, exists := e.tools[toolName]
	if !exists {
		return nil, &ToolError{
			Code:    "tool_not_found",
			Message: fmt.Sprintf("Tool %q not found", toolName),
		}
	}

	// Create a context with timeout
	execCtx, cancel := context.WithTimeout(ctx, e.timeout)
	defer cancel()

	// Log the execution
	log.Debug(fmt.Sprintf("[TOOL] Execute: %s", toolName),
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

	log.Debug(fmt.Sprintf("[TOOL] Complete: %s (%s)", toolName, duration.Round(time.Millisecond)))

	return result, nil
}
