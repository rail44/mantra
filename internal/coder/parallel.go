package coder

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"log/slog"

	"golang.org/x/sync/errgroup"

	"github.com/rail44/mantra/internal/config"
	"github.com/rail44/mantra/internal/llm"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/parser"
	"github.com/rail44/mantra/internal/phase"
	"github.com/rail44/mantra/internal/ui"
)

// ParallelCoder handles parallel code generation for multiple targets
type ParallelCoder struct {
	client *llm.Client
	config *config.Config
	logger log.Logger
}

// NewParallelCoder creates a new parallel coder
func NewParallelCoder(client *llm.Client, cfg *config.Config) *ParallelCoder {
	return &ParallelCoder{
		client: client,
		config: cfg,
		logger: log.Default(),
	}
}

// TargetContext contains a target and its associated file context
type TargetContext struct {
	Target      *parser.Target
	FileContent string
	FileInfo    *parser.FileInfo
}

// ExecuteTargets generates implementations for all targets in parallel
func (c *ParallelCoder) ExecuteTargets(ctx context.Context, targets []TargetContext) ([]*parser.GenerationResult, error) {
	if len(targets) == 0 {
		return []*parser.GenerationResult{}, nil
	}

	// Get project root from the first target's file path
	projectRoot := findProjectRoot(filepath.Dir(targets[0].Target.FilePath))

	// Create TUI program for parallel execution
	uiProgram := ui.NewProgram()

	// Thread-safe collections for collecting results
	var mu sync.Mutex
	var allResults []*parser.GenerationResult

	// Channel to signal completion
	done := make(chan struct{})

	// Start generation in background
	go func() {
		// Use errgroup with limited concurrency (max 16)
		g, ctx := errgroup.WithContext(ctx)
		g.SetLimit(16)

		// Process each target in parallel
		for i, tc := range targets {
			// Capture loop variables
			index := i + 1
			total := len(targets)
			targetCtx := tc

			g.Go(func() error {
				result := c.generateSingleTarget(ctx, targetCtx, projectRoot, index, total, uiProgram)

				mu.Lock()
				allResults = append(allResults, result)
				mu.Unlock()

				return nil
			})
		}

		// Wait for all goroutines to complete
		g.Wait()

		// Signal completion
		close(done)
	}()

	// Wait for completion or TUI to finish
	go func() {
		<-done
		time.Sleep(100 * time.Millisecond) // Allow final render
		uiProgram.Quit()
	}()

	// Run TUI (blocks until Quit is called)
	if err := uiProgram.Start(); err != nil {
		<-done // Ensure generation completes even if TUI fails
	}

	// Display logs based on verbose flag
	c.displayLogs(uiProgram)

	return allResults, nil
}

// generateSingleTarget generates implementation for a single target
func (c *ParallelCoder) generateSingleTarget(ctx context.Context, tc TargetContext, projectRoot string, targetNum, totalTargets int, uiProgram *ui.Program) *parser.GenerationResult {
	targetStart := time.Now()

	// Create a target-specific logger with display name
	logger := uiProgram.CreateTargetLogger(tc.Target.GetDisplayName(), targetNum, totalTargets)

	// Use the shared client directly instead of creating a new one
	// This enables HTTP connection reuse across all targets
	// Note: This means all targets share the same logger, but target-specific
	// logging is handled by the UI program

	// Create phase runner with shared client
	runner := phase.NewRunner(c.client, logger)

	// Phase 1: Context Gathering
	contextResult, contextError := runner.ExecuteContextGathering(ctx, tc.Target, tc.FileContent, targetNum, uiProgram)
	if contextError != nil {
		return c.createPhaseFailure(tc.Target, contextError, targetStart, targetNum, uiProgram)
	}

	// Phase 2: Implementation
	implementation, implError := runner.ExecuteImplementation(ctx, tc.Target, tc.FileContent, tc.FileInfo, projectRoot, contextResult, targetNum, uiProgram)
	if implError != nil {
		return c.createPhaseFailure(tc.Target, implError, targetStart, targetNum, uiProgram)
	}

	// Success
	duration := time.Since(targetStart).Round(time.Millisecond)
	logger.Info("Successfully generated implementation", "duration", duration)
	uiProgram.Complete(targetNum)

	return &parser.GenerationResult{
		Target:         tc.Target,
		Success:        true,
		Implementation: implementation,
		Duration:       duration,
	}
}

// createInitializationFailure creates a failure result for initialization errors
func (c *ParallelCoder) createInitializationFailure(target *parser.Target, err error, startTime time.Time, targetNum int, uiProgram *ui.Program) *parser.GenerationResult {
	duration := time.Since(startTime).Round(time.Millisecond)
	failureReason := &parser.FailureReason{
		Phase:   "initialization",
		Message: fmt.Sprintf("Failed to create AI client: %v", err),
		Context: "Check your API configuration and network connection",
	}

	uiProgram.Fail(targetNum)
	c.logger.Error("Failed to create AI client",
		slog.String("target", target.GetDisplayName()),
		slog.String("error", err.Error()))

	return &parser.GenerationResult{
		Target:        target,
		Success:       false,
		FailureReason: failureReason,
		Duration:      duration,
	}
}

// createPhaseFailure creates a failure result for phase execution errors
func (c *ParallelCoder) createPhaseFailure(target *parser.Target, failureReason *parser.FailureReason, startTime time.Time, targetNum int, uiProgram *ui.Program) *parser.GenerationResult {
	duration := time.Since(startTime).Round(time.Millisecond)
	uiProgram.Fail(targetNum)

	c.logger.Error("Phase execution failed",
		slog.String("target", target.GetDisplayName()),
		slog.String("phase", failureReason.Phase),
		slog.String("message", failureReason.Message))

	return &parser.GenerationResult{
		Target:        target,
		Success:       false,
		FailureReason: failureReason,
		Duration:      duration,
	}
}

// displayLogs displays logs based on configuration
func (c *ParallelCoder) displayLogs(uiProgram *ui.Program) {
	if c.config.Verbose {
		c.displayAllTargetLogs(uiProgram)
	} else {
		c.displayFailedTargetLogs(uiProgram)
	}
}

// displayAllTargetLogs displays logs for all targets
func (c *ParallelCoder) displayAllTargetLogs(uiProgram *ui.Program) {
	// Display all captured logs for all targets
	allTargets := uiProgram.GetAllTargets()
	for _, target := range allTargets {
		logs := target.GetAllLogs()
		if len(logs) > 0 {
			c.logger.Info(fmt.Sprintf("=== Logs for %s ===", target.Name))
			for _, logEntry := range logs {
				// Re-emit each log entry at appropriate level
				switch logEntry.Level {
				case "TRACE":
					c.logger.Trace(logEntry.Message)
				case "DEBUG":
					c.logger.Debug(logEntry.Message)
				case "INFO":
					c.logger.Info(logEntry.Message)
				case "WARN":
					c.logger.Warn(logEntry.Message)
				case "ERROR":
					c.logger.Error(logEntry.Message)
				default:
					c.logger.Info(logEntry.Message)
				}
			}
		}
	}
}

// displayFailedTargetLogs displays logs only for failed targets
func (c *ParallelCoder) displayFailedTargetLogs(uiProgram *ui.Program) {
	// Display logs only for failed targets
	failedTargets := uiProgram.GetFailedTargets()
	if len(failedTargets) > 0 {
		c.logger.Info("=== Logs for failed targets ===")
		for _, target := range failedTargets {
			logs := target.GetAllLogs()
			if len(logs) > 0 {
				c.logger.Info(fmt.Sprintf("--- %s ---", target.Name))
				for _, logEntry := range logs {
					// Re-emit each log entry at appropriate level
					switch logEntry.Level {
					case "TRACE":
						c.logger.Trace(logEntry.Message)
					case "DEBUG":
						c.logger.Debug(logEntry.Message)
					case "INFO":
						c.logger.Info(logEntry.Message)
					case "WARN":
						c.logger.Warn(logEntry.Message)
					case "ERROR":
						c.logger.Error(logEntry.Message)
					default:
						c.logger.Info(logEntry.Message)
					}
				}
			}
		}
	}
}

// findProjectRoot finds the project root by looking for go.mod
func findProjectRoot(startDir string) string {
	dir := startDir
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			// Reached filesystem root
			return startDir
		}
		dir = parent
	}
}
