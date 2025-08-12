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
		Plain:    c.config.Plain,
		LogLevel: log.GetCurrentLevel(),
	})

	// Create event channel for phase updates
	eventCh := make(chan phase.TargetEvent, 100)

	// Start UI event processor
	go func() {
		for event := range eventCh {
			uiProgram.UpdatePhase(event.TargetIndex, event.Phase, event.Step)
		}
	}()

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
				// Register target with UI
				uiProgram.AddTarget(targetCtx.Target.GetDisplayName(), index, total)

				// Create event callback for this target
				eventCallback := func(phaseName, step string) {
					eventCh <- phase.TargetEvent{
						TargetIndex: index,
						Phase:       phaseName,
						Step:        step,
						Time:        time.Now(),
					}
				}

				// Create target callbacks based on UI mode
				var targetCallbacks TargetCallbacks
				if uiProgram.IsTUIEnabled() {
					// TUI mode: use all callbacks
					targetCallbacks = TargetCallbacks{
						SendLog: func(targetNum int, level slog.Level, message string) {
							uiProgram.SendLog(targetNum, level, message)
						},
						MarkRunning: func(targetNum int) {
							uiProgram.MarkAsRunning(targetNum)
						},
						Complete: func(targetNum int) {
							uiProgram.Complete(targetNum)
						},
						Fail: func(targetNum int) {
							uiProgram.Fail(targetNum)
						},
					}
				} else {
					// Plain mode: SendLog is nil (PlainLogger handles output directly)
					// Other callbacks are used for state tracking
					targetCallbacks = TargetCallbacks{
						SendLog: nil, // PlainLogger handles output directly
						MarkRunning: func(targetNum int) {
							uiProgram.MarkAsRunning(targetNum)
						},
						Complete: func(targetNum int) {
							uiProgram.Complete(targetNum)
						},
						Fail: func(targetNum int) {
							uiProgram.Fail(targetNum)
						},
					}
				}

				result := c.generateSingleTargetWithCallback(ctx, targetCtx, projectRoot, index, total, targetCallbacks, eventCallback)

				mu.Lock()
				allResults = append(allResults, result)
				mu.Unlock()

				return nil
			})
		}

		// Wait for all goroutines to complete
		g.Wait()

		// Close event channel
		close(eventCh)

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
		// Wait for TUI to actually finish if it's enabled
		if uiProgram.IsTUIEnabled() {
			<-tuiDone
		}
	case err := <-tuiDone:
		// TUI finished (shouldn't happen normally)
		// Don't log here as it might corrupt the display
		// Store the error for later if needed
		<-done // Still wait for generation to complete
		if err != nil {
			// Log after everything is done
			c.logger.Debug("TUI error", "error", err)
		}
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

// TargetCallbacks contains callbacks for target lifecycle events
type TargetCallbacks struct {
	SendLog     func(targetNum int, level slog.Level, message string)
	MarkRunning func(targetNum int)
	Complete    func(targetNum int)
	Fail        func(targetNum int)
}

// generateSingleTargetWithCallback generates implementation for a single target with event callback
func (c *ParallelCoder) generateSingleTargetWithCallback(ctx context.Context, tc TargetContext, projectRoot string, targetNum, totalTargets int, callbacks TargetCallbacks, eventCallback func(string, string)) *parser.GenerationResult {
	targetStart := time.Now()

	// Create appropriate logger based on UI mode
	var logger log.Logger
	if callbacks.SendLog != nil {
		// TUI mode: use callback logger to send logs to UI
		logger = log.NewCallbackLoggerFromHandler(func(level slog.Level, msg string, args []any) {
			// Format message with args
			formattedMsg := msg
			if len(args) > 0 {
				// Simple formatting for structured attributes
				for i := 0; i < len(args)-1; i += 2 {
					formattedMsg += fmt.Sprintf(" %v=%v", args[i], args[i+1])
				}
			}
			callbacks.SendLog(targetNum, level, formattedMsg)
		}, log.GetCurrentLevel())
	} else {
		// Plain mode: use PlainLogger for direct output
		logger = log.NewPlainLogger(targetNum, totalTargets, tc.Target.GetDisplayName(), os.Stderr, log.GetCurrentLevel())
	}

	// Log generation start
	logger.Info("Starting generation")

	// Explicitly mark target as running now that we're starting execution
	callbacks.MarkRunning(targetNum)

	// Create a new llm.Client for this target with shared HTTP client
	// This ensures each target has independent state (temperature, systemPrompt)
	// while still benefiting from connection pooling
	client, err := llm.NewClient(c.clientConfig, c.httpClient, logger)
	if err != nil {
		return c.createInitializationFailure(tc.Target, err, targetStart, targetNum, callbacks.Fail)
	}

	// Create phase runner with target-specific client
	runner := phase.NewRunner(client, logger)

	// Phase 1: Context Gathering
	// Pass destination directory for PackageLoader to use prepared stub files
	contextStepCallback := func(step string) {
		eventCallback(phase.PhaseContextGathering, step)
	}
	contextResult, contextError := runner.ExecuteContextGathering(ctx, tc.Target, tc.FileContent, c.config.Dest, contextStepCallback)
	if contextError != nil {
		return c.createPhaseFailure(tc.Target, contextError, targetStart, targetNum, callbacks.Fail)
	}

	// Phase 2: Implementation
	implStepCallback := func(step string) {
		eventCallback(phase.PhaseImplementation, step)
	}
	implementation, implError := runner.ExecuteImplementation(ctx, tc.Target, tc.FileContent, tc.FileInfo, projectRoot, contextResult, implStepCallback)
	if implError != nil {
		return c.createPhaseFailure(tc.Target, implError, targetStart, targetNum, callbacks.Fail)
	}

	// Success
	duration := time.Since(targetStart).Round(time.Millisecond)
	// Log through the target logger which will send to TUI
	logger.Info("Successfully generated implementation", "duration", duration)
	callbacks.Complete(targetNum)

	return &parser.GenerationResult{
		Target:         tc.Target,
		Success:        true,
		Implementation: implementation,
		Duration:       duration,
	}
}

// createInitializationFailure creates a failure result for initialization errors
func (c *ParallelCoder) createInitializationFailure(target *parser.Target, err error, startTime time.Time, targetNum int, failCallback func(int)) *parser.GenerationResult {
	duration := time.Since(startTime).Round(time.Millisecond)
	failureReason := &parser.FailureReason{
		Phase:   "initialization",
		Message: fmt.Sprintf("Failed to create AI client: %v", err),
		Context: "Check your API configuration and network connection",
	}

	failCallback(targetNum)
	// Don't log directly during TUI execution - it corrupts the display
	// The error will be shown in the TUI and logged after completion

	return &parser.GenerationResult{
		Target:        target,
		Success:       false,
		FailureReason: failureReason,
		Duration:      duration,
	}
}

// createPhaseFailure creates a failure result for phase execution errors
func (c *ParallelCoder) createPhaseFailure(target *parser.Target, failureReason *parser.FailureReason, startTime time.Time, targetNum int, failCallback func(int)) *parser.GenerationResult {
	duration := time.Since(startTime).Round(time.Millisecond)
	failCallback(targetNum)
	// Don't log directly during TUI execution - it corrupts the display
	// The error will be shown in the TUI and logged after completion

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
			// Show logs for failed targets (already filtered by log level)
			for _, target := range failedTargets {
				logs := target.GetAllLogs()
				for _, logEntry := range logs {
					// No manual filtering needed - logs are already filtered by TargetLogger
					c.reEmitLogEntry(logEntry, target.Name)
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
						// Re-emit each log entry at appropriate level (already filtered)
						c.reEmitLogEntry(logEntry, target.Name)
					}
				}
			}
		}
	}
}

// reEmitLogEntry re-emits a log entry at the appropriate level
func (c *ParallelCoder) reEmitLogEntry(logEntry ui.LogEntry, targetName string) {
	message := fmt.Sprintf("[%s] %s", targetName, logEntry.Message)

	switch logEntry.Level {
	case slog.LevelDebug - 4: // TRACE
		c.logger.Trace(message)
	case slog.LevelDebug:
		c.logger.Debug(message)
	case slog.LevelInfo:
		c.logger.Info(message)
	case slog.LevelWarn:
		c.logger.Warn(message)
	case slog.LevelError:
		c.logger.Error(message)
	default:
		c.logger.Info(message)
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
