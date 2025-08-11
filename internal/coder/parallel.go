package coder

import (
	"context"
	"fmt"
	"net/http"
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
	clientConfig *llm.ClientConfig
	config       *config.Config
	logger       log.Logger
	httpClient   *http.Client // Shared HTTP client for connection pooling
}

// NewParallelCoder creates a new parallel coder
func NewParallelCoder(clientConfig *llm.ClientConfig, cfg *config.Config) *ParallelCoder {
	return &ParallelCoder{
		clientConfig: clientConfig,
		config:       cfg,
		logger:       log.Default(),
		httpClient: &http.Client{
			Timeout: 5 * time.Minute,
		},
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
	// Use plain console output if --plain flag is set
	uiProgram := ui.NewProgramWithOptions(ui.ProgramOptions{
		Plain: c.config.Plain,
	})

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

	// Start TUI in background
	tuiDone := make(chan error, 1)
	go func() {
		tuiDone <- uiProgram.Start()
	}()

	// Wait for either TUI to finish or generation to complete
	select {
	case <-done:
		// Generation completed, TUI will be quit automatically
	case err := <-tuiDone:
		// TUI finished (shouldn't happen normally)
		if err != nil {
			c.logger.Debug("TUI error", "error", err)
		}
		<-done // Still wait for generation to complete
	}

	// Display logs for failed targets
	// TUI mode already shows progress, so only display failures
	// Plain mode shows simple progress, so also only display failures
	if uiProgram.IsTUIEnabled() {
		// Add a newline after TUI to ensure clean output
		fmt.Fprintln(os.Stderr, "")
	}
	c.displayFailedTargetLogs(uiProgram)

	return allResults, nil
}

// generateSingleTarget generates implementation for a single target
func (c *ParallelCoder) generateSingleTarget(ctx context.Context, tc TargetContext, projectRoot string, targetNum, totalTargets int, uiProgram *ui.Program) *parser.GenerationResult {
	targetStart := time.Now()

	// Create a target-specific logger with display name
	logger := uiProgram.CreateTargetLogger(tc.Target.GetDisplayName(), targetNum, totalTargets)

	// Create a new llm.Client for this target with shared HTTP client
	// This ensures each target has independent state (temperature, systemPrompt)
	// while still benefiting from connection pooling
	client, err := llm.NewClient(c.clientConfig, c.httpClient, logger)
	if err != nil {
		return c.createInitializationFailure(tc.Target, err, targetStart, targetNum, uiProgram)
	}

	// Create phase runner with target-specific client
	runner := phase.NewRunner(client, logger)

	// Phase 1: Context Gathering
	// Pass destination directory for PackageLoader to use prepared stub files
	contextResult, contextError := runner.ExecuteContextGathering(ctx, tc.Target, tc.FileContent, c.config.Dest, targetNum, uiProgram)
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

// displayFailedTargetLogs displays logs only for failed targets
func (c *ParallelCoder) displayFailedTargetLogs(uiProgram *ui.Program) {
	// Display logs only for failed targets
	failedTargets := uiProgram.GetFailedTargets()
	if len(failedTargets) > 0 {
		// In plain output mode, be more concise
		if !uiProgram.IsTUIEnabled() {
			// Only show ERROR level logs for plain output mode
			for _, target := range failedTargets {
				logs := target.GetAllLogs()
				for _, logEntry := range logs {
					if logEntry.Level == "ERROR" {
						c.logger.Error(fmt.Sprintf("[%s] %s", target.Name, logEntry.Message))
					}
				}
			}
		} else {
			// Terminal mode: show all logs as before
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
