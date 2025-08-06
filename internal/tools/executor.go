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
	logger  log.Logger
}

// NewExecutor creates a new tool executor
func NewExecutor(tools []Tool, logger log.Logger) *Executor {
	if logger == nil {
		logger = log.Default()
	}

	toolMap := make(map[string]Tool)
	for _, tool := range tools {
		toolMap[tool.Name()] = tool
	}
	return &Executor{
		tools:   toolMap,
		timeout: 30 * time.Second, // Default timeout
		logger:  logger,
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

	// Log the tool execution in a user-friendly way
	switch toolName {
	case "search":
		if pattern, ok := params["pattern"].(string); ok {
			e.logger.Debug(fmt.Sprintf("Searching for: %s", pattern))
		}
	case "inspect":
		if symbol, ok := params["symbol"].(string); ok {
			e.logger.Debug(fmt.Sprintf("Inspecting symbol: %s", symbol))
		}
	case "read_func":
		if name, ok := params["name"].(string); ok {
			e.logger.Debug(fmt.Sprintf("Reading function: %s", name))
		}
	case "check_syntax":
		e.logger.Debug("Validating generated code syntax")
	default:
		e.logger.Debug(fmt.Sprintf("Executing tool: %s", toolName))
	}

	// Execute the tool
	start := time.Now()
	result, err := tool.Execute(execCtx, params)
	duration := time.Since(start)

	// Log the result
	if err != nil {
		e.logger.Error(fmt.Sprintf("Tool '%s' failed", toolName),
			slog.String("error", err.Error()),
			slog.Duration("duration", duration))
		return nil, err
	}

	e.logger.Trace(fmt.Sprintf("Tool '%s' completed (%s)", toolName, duration.Round(time.Millisecond)))

	return result, nil
}
