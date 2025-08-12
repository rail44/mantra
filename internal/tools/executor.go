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
	context *Context // Shared context for tools
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
		context: nil, // Will be set via SetContext if needed
	}
}

// SetContext sets the shared context for tools
func (e *Executor) SetContext(ctx *Context) {
	e.context = ctx
}

// IsTerminal checks if a tool is terminal by name
func (e *Executor) IsTerminal(toolName string) bool {
	if tool, exists := e.tools[toolName]; exists {
		return tool.IsTerminal()
	}
	return false
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

	// Log the tool execution and update UI phase details
	var phaseDetail string
	switch toolName {
	case "search":
		if pattern, ok := params["pattern"].(string); ok {
			phaseDetail = fmt.Sprintf("Searching: %s", pattern)
			e.logger.Debug(phaseDetail)
		}
	case "inspect":
		if symbol, ok := params["symbol"].(string); ok {
			phaseDetail = fmt.Sprintf("Inspecting: %s", symbol)
			e.logger.Debug(phaseDetail)
		}
	case "read_func":
		if name, ok := params["name"].(string); ok {
			phaseDetail = fmt.Sprintf("Reading: %s", name)
			e.logger.Debug(phaseDetail)
		}
	case "check_code":
		phaseDetail = "Validating code"
		e.logger.Debug(phaseDetail)
	default:
		phaseDetail = fmt.Sprintf("Running: %s", toolName)
		e.logger.Debug(phaseDetail)
	}

	// Update UI phase detail if available
	if e.context != nil && e.context.UIProgram != nil && phaseDetail != "" {
		// Determine current phase based on tool
		currentPhase := "Context Gathering"
		if toolName == "check_code" || toolName == "result" {
			currentPhase = "Implementation"
		}
		e.context.UIProgram.UpdatePhase(e.context.TargetNum, currentPhase, phaseDetail)
	}

	// If the tool implements ContextAwareTool and we have context, provide it
	if e.context != nil {
		if contextAware, ok := tool.(ContextAwareTool); ok {
			contextAware.SetContext(e.context)
		}
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
