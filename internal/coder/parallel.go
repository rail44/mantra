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
	logger       *slog.Logger
	httpClient   *http.Client // Shared HTTP client for connection pooling
}

// NewParallelCoder creates a new parallel coder
func NewParallelCoder(clientConfig *llm.ClientConfig, cfg *config.Config) *ParallelCoder {
	return &ParallelCoder{
		clientConfig: clientConfig,
		config:       cfg,
		logger:       slog.Default(),
		httpClient: &http.Client{
			Timeout: 5 * time.Minute,
		},
	}
}

// TargetContext contains a target and its associated file context
type TargetContext struct {
	Target      *parser.Target
	Index       int
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

	uiProgram := ui.NewProgramWithOptions(ui.ProgramOptions{
		Plain: c.config.Plain,
	})

	// Thread-safe collections for collecting results
	var mu sync.Mutex
	var allResults []*parser.GenerationResult

	// Start TUI in background
	tuiDone := make(chan *ui.Model, 1)
	go func() {
		model, _ := uiProgram.Start()
		tuiDone <- model
	}()

	g, ctx := errgroup.WithContext(ctx)
	g.SetLimit(16)

	// Process each target in parallel
	for _, tc := range targets {
		g.Go(func() error {
			// Register target with UI
			uiProgram.AddTarget(tc.Target.GetDisplayName(), tc.Index, len(targets))

			handler := log.NewCallbackHandler(
				uiProgram.SendLog,
			).WithAttrs([]slog.Attr{
				slog.Int("targetIndex", tc.Index),
				slog.Int("totalTargets", len(targets)),
				slog.String("targetName", tc.Target.GetDisplayName()),
			})

			coder := NewTargetCoder(ctx, c, tc, projectRoot, slog.New(handler), uiProgram)
			result := coder.Generate()

			mu.Lock()
			allResults = append(allResults, result)
			mu.Unlock()
			return nil
		})
	}

	g.Wait()

	// Stop the UI
	time.Sleep(100 * time.Millisecond) // Allow final render
	uiProgram.Quit()

	// Wait for UI to finish and get final model
	finalModel := <-tuiDone

	// Display logs for failed targets
	// Only needed in TUI mode where logs are captured
	// In plain mode, logs are already displayed in real-time
	if finalModel.IsTUIEnabled() {
		c.displayFailedTargetLogs(ctx, finalModel)
	}

	return allResults, nil
}

// TargetCoder handles the code generation for a single target
type TargetCoder struct {
	ctx         context.Context
	coder       *ParallelCoder
	target      TargetContext
	projectRoot string
	uiProgram   *ui.Program
	logger      *slog.Logger
}

// NewTargetCoder creates a new target coder
func NewTargetCoder(ctx context.Context, coder *ParallelCoder, target TargetContext, projectRoot string, logger *slog.Logger, uiProgram *ui.Program) *TargetCoder {
	return &TargetCoder{
		ctx:         ctx,
		coder:       coder,
		target:      target,
		projectRoot: projectRoot,
		uiProgram:   uiProgram,
		logger:      logger,
	}
}

// Generate executes the code generation process for the target
func (t *TargetCoder) Generate() *parser.GenerationResult {
	startTime := time.Now()

	// Log generation start
	t.logger.Info("Starting generation")

	// Mark target as running
	t.markRunning()

	// Create LLM client
	client, err := t.createClient()
	if err != nil {
		return t.failureResult(startTime, "initialization", fmt.Sprintf("Failed to create AI client: %v", err), "Check your API configuration and network connection")
	}

	// Execute phases
	runner := phase.NewRunner(client, t.logger)

	// Phase 1: Context Gathering
	contextResult, failureReason := t.executeContextGathering(runner)
	if failureReason != nil {
		return t.phaseFailureResult(startTime, failureReason)
	}

	// Phase 2: Implementation
	implementation, failureReason := t.executeImplementation(runner, contextResult)
	if failureReason != nil {
		return t.phaseFailureResult(startTime, failureReason)
	}

	// Success
	return t.successResult(startTime, implementation)
}

// createClient creates a new LLM client for this target
func (t *TargetCoder) createClient() (*llm.Client, error) {
	return llm.NewClient(t.coder.clientConfig, t.coder.httpClient, t.logger)
}

// executeContextGathering executes the context gathering phase
func (t *TargetCoder) executeContextGathering(runner *phase.Runner) (map[string]any, *parser.FailureReason) {
	return runner.ExecuteContextGathering(t.ctx, t.target.Target, t.target.FileContent, t.coder.config.Dest)
}

// executeImplementation executes the implementation phase
func (t *TargetCoder) executeImplementation(runner *phase.Runner, contextResult map[string]any) (string, *parser.FailureReason) {
	return runner.ExecuteImplementation(t.ctx, t.target.Target, t.target.FileContent, t.target.FileInfo, t.projectRoot, contextResult)
}

// successResult creates a successful generation result
func (t *TargetCoder) successResult(startTime time.Time, implementation string) *parser.GenerationResult {
	duration := time.Since(startTime).Round(time.Millisecond)
	t.logger.Info("Successfully generated implementation", "duration", duration)
	t.markComplete()

	return &parser.GenerationResult{
		Target:         t.target.Target,
		Success:        true,
		Implementation: implementation,
		Duration:       duration,
	}
}

// failureResult creates a failure result
func (t *TargetCoder) failureResult(startTime time.Time, phase, message, context string) *parser.GenerationResult {
	t.markFailed()
	return &parser.GenerationResult{
		Target:  t.target.Target,
		Success: false,
		FailureReason: &parser.FailureReason{
			Phase:   phase,
			Message: message,
			Context: context,
		},
		Duration: time.Since(startTime).Round(time.Millisecond),
	}
}

// phaseFailureResult creates a failure result from a phase error
func (t *TargetCoder) phaseFailureResult(startTime time.Time, failureReason *parser.FailureReason) *parser.GenerationResult {
	t.markFailed()
	return &parser.GenerationResult{
		Target:        t.target.Target,
		Success:       false,
		FailureReason: failureReason,
		Duration:      time.Since(startTime).Round(time.Millisecond),
	}
}

// UI callback methods

// markRunning marks the target as running
func (t *TargetCoder) markRunning() {
	t.uiProgram.MarkAsRunning(t.target.Index)
}

// markComplete marks the target as complete
func (t *TargetCoder) markComplete() {
	t.uiProgram.Complete(t.target.Index)
}

// markFailed marks the target as failed
func (t *TargetCoder) markFailed() {
	t.uiProgram.Fail(t.target.Index)
}

// displayFailedTargetLogs displays logs only for failed targets in TUI mode
func (c *ParallelCoder) displayFailedTargetLogs(ctx context.Context, model *ui.Model) {
	failedTargets := model.GetFailedTargets()
	if len(failedTargets) == 0 {
		return
	}

	fmt.Fprintln(os.Stderr, "")

	c.logger.Info("=== Logs for failed targets ===")
	for _, target := range failedTargets {
		logs := target.GetAllLogs()
		if len(logs) == 0 {
			continue
		}

		c.logger.Info(fmt.Sprintf("--- %s ---", target.Name))
		for _, record := range logs {
			c.logger.Handler().Handle(ctx, record)
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
