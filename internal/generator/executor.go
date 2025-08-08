package generator

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"golang.org/x/sync/errgroup"

	"github.com/rail44/mantra/internal/ai"
	"github.com/rail44/mantra/internal/config"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/parser"
	"github.com/rail44/mantra/internal/phase"
	"github.com/rail44/mantra/internal/ui"
)

// TargetExecutor handles parallel execution of target generation
type TargetExecutor struct {
	client *ai.Client
	config *config.Config
	logger log.Logger
}

// NewTargetExecutor creates a new target executor
func NewTargetExecutor(client *ai.Client, cfg *config.Config) *TargetExecutor {
	return &TargetExecutor{
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

// ExecuteTargetsInParallel generates implementations for all targets in parallel
func (e *TargetExecutor) ExecuteTargetsInParallel(ctx context.Context, targets []TargetContext) ([]*parser.GenerationResult, error) {
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
				result := e.generateSingleTarget(ctx, targetCtx, projectRoot, index, total, uiProgram)

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

	// Stop TUI after completion
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
	e.displayLogs(uiProgram)

	return allResults, nil
}

// generateSingleTarget generates implementation for a single target
func (e *TargetExecutor) generateSingleTarget(ctx context.Context, tc TargetContext, projectRoot string, targetNum, totalTargets int, uiProgram *ui.Program) *parser.GenerationResult {
	targetStart := time.Now()

	// Create a target-specific logger with display name
	logger := uiProgram.CreateTargetLogger(tc.Target.GetDisplayName(), targetNum, totalTargets)

	// Create a new AI client with the target-specific logger
	aiClient, err := ai.NewClient(e.client.GetConfig(), logger)
	if err != nil {
		return e.createInitializationFailure(tc.Target, err, targetStart, targetNum, uiProgram)
	}

	// Create phase runner
	runner := phase.NewRunner(aiClient, logger)

	// Phase 1: Context Gathering
	contextResult, contextError := runner.ExecuteContextGathering(ctx, tc.Target, tc.FileContent, targetNum, uiProgram)
	if contextError != nil {
		return e.createPhaseFailure(tc.Target, contextError, targetStart, targetNum, uiProgram)
	}

	// Phase 2: Implementation
	implementation, implError := runner.ExecuteImplementation(ctx, tc.Target, tc.FileContent, tc.FileInfo, projectRoot, contextResult, targetNum, uiProgram)
	if implError != nil {
		return e.createPhaseFailure(tc.Target, implError, targetStart, targetNum, uiProgram)
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
func (e *TargetExecutor) createInitializationFailure(target *parser.Target, err error, startTime time.Time, targetNum int, uiProgram *ui.Program) *parser.GenerationResult {
	e.logger.Error("Failed to create AI client", "error", err.Error())
	uiProgram.Fail(targetNum)
	return &parser.GenerationResult{
		Target:  target,
		Success: false,
		FailureReason: &parser.FailureReason{
			Phase:   "initialization",
			Message: "Failed to create AI client: " + err.Error(),
			Context: "Client configuration error",
		},
		Duration: time.Since(startTime),
	}
}

// createPhaseFailure creates a failure result for phase errors
func (e *TargetExecutor) createPhaseFailure(target *parser.Target, failureReason *parser.FailureReason, startTime time.Time, targetNum int, uiProgram *ui.Program) *parser.GenerationResult {
	uiProgram.Fail(targetNum)
	return &parser.GenerationResult{
		Target:        target,
		Success:       false,
		FailureReason: failureReason,
		Duration:      time.Since(startTime),
	}
}

// displayLogs displays execution logs based on verbosity settings
func (e *TargetExecutor) displayLogs(uiProgram *ui.Program) {
	if verbose := e.config.Verbose; verbose {
		// Show logs for all targets when verbose is enabled
		e.displayAllTargetLogs(uiProgram)
	} else {
		// Show logs only for failed targets when not verbose
		e.displayFailedTargetLogs(uiProgram)
	}
}

// displayAllTargetLogs displays logs for all targets
func (e *TargetExecutor) displayAllTargetLogs(uiProgram *ui.Program) {
	allTargets := uiProgram.GetAllTargets()
	if len(allTargets) == 0 {
		return
	}

	fmt.Println()
	e.logger.Info("Detailed logs for all targets")
	fmt.Println()

	for _, target := range allTargets {
		status := "Success"
		if target.Status == "failed" {
			status = "Failed"
		}
		e.logger.Info(fmt.Sprintf("%s: %s", status, target.Name),
			"duration", target.EndTime.Sub(target.StartTime).Round(time.Millisecond).String())

		logs := target.GetAllLogs()
		for _, logEntry := range logs {
			timestamp := logEntry.Timestamp.Format("15:04:05.000")
			fmt.Printf("  [%s] %s: %s\n", timestamp, logEntry.Level, logEntry.Message)
		}
		fmt.Println()
	}
}

// displayFailedTargetLogs displays logs for failed targets only
func (e *TargetExecutor) displayFailedTargetLogs(uiProgram *ui.Program) {
	failedTargets := uiProgram.GetFailedTargets()
	if len(failedTargets) == 0 {
		return
	}

	fmt.Println()
	e.logger.Error("Failed targets - Detailed logs")
	fmt.Println()

	for _, target := range failedTargets {
		e.logger.Error(fmt.Sprintf("Failed: %s", target.Name),
			"duration", target.EndTime.Sub(target.StartTime).Round(time.Millisecond).String())

		logs := target.GetAllLogs()
		for _, logEntry := range logs {
			timestamp := logEntry.Timestamp.Format("15:04:05.000")
			fmt.Printf("  [%s] %s: %s\n", timestamp, logEntry.Level, logEntry.Message)
		}
		fmt.Println()
	}

	e.logger.Error("Total failures", "count", len(failedTargets))
}

// findProjectRoot finds the project root by looking for go.mod
func findProjectRoot(startPath string) string {
	projectRoot := startPath
	for {
		if _, err := os.Stat(filepath.Join(projectRoot, "go.mod")); err == nil {
			return projectRoot
		}
		parent := filepath.Dir(projectRoot)
		if parent == projectRoot {
			// Reached root without finding go.mod
			return startPath
		}
		projectRoot = parent
	}
}
