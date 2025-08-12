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

	// Setup UI and event handling
	uiProgram, eventCh := c.setupUI()

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
			index := i + 1
			targetCtx := tc

			g.Go(func() error {
				result := c.processTarget(ctx, targetCtx, projectRoot, index, len(targets), uiProgram, eventCh)
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

	// Wait for generation and UI to complete
	c.waitForCompletion(done, uiProgram)

	// Display logs for failed targets
	// TUI mode already shows progress, so only display failures
	// Plain mode shows simple progress, so also only display failures
	if uiProgram.IsTUIEnabled() {
		// Add a newline after TUI to ensure clean output
		fmt.Fprintln(os.Stderr, "")
	}
	c.displayFailedTargetLogs(ctx, uiProgram)

	return allResults, nil
}

// TargetCallbacks contains callbacks for target lifecycle events
type TargetCallbacks struct {
	SendLog     func(record slog.Record)
	MarkRunning func(targetNum int)
	Complete    func(targetNum int)
	Fail        func(targetNum int)
}

// TargetCoder handles the code generation for a single target
type TargetCoder struct {
	coder        *ParallelCoder
	ctx          context.Context
	target       TargetContext
	projectRoot  string
	index        int
	total        int
	callbacks    TargetCallbacks
	eventCallback func(string, string)
	logger       *slog.Logger
	startTime    time.Time
}

// NewTargetCoder creates a new target coder
func NewTargetCoder(coder *ParallelCoder, ctx context.Context, target TargetContext, projectRoot string, index, total int, callbacks TargetCallbacks, eventCallback func(string, string)) *TargetCoder {
	// Create a callback handler with target attributes
	callbackHandler := log.NewCallbackHandler(
		callbacks.SendLog,
	).WithAttrs([]slog.Attr{
		slog.Int("targetIndex", index),
		slog.Int("totalTargets", total),
		slog.String("targetName", target.Target.GetDisplayName()),
	})

	return &TargetCoder{
		coder:         coder,
		ctx:           ctx,
		target:        target,
		projectRoot:   projectRoot,
		index:         index,
		total:         total,
		callbacks:     callbacks,
		eventCallback: eventCallback,
		logger:        slog.New(callbackHandler),
		startTime:     time.Now(),
	}
}

// Generate executes the code generation process for the target
func (g *TargetCoder) Generate() *parser.GenerationResult {
	// Log generation start
	g.logger.Info("Starting generation")

	// Mark target as running
	g.callbacks.MarkRunning(g.index)

	// Create LLM client
	client, err := g.createClient()
	if err != nil {
		return g.failureResult("initialization", fmt.Sprintf("Failed to create AI client: %v", err), "Check your API configuration and network connection")
	}

	// Execute phases
	runner := phase.NewRunner(client, g.logger)

	// Phase 1: Context Gathering
	contextResult, failureReason := g.executeContextGathering(runner)
	if failureReason != nil {
		return g.phaseFailureResult(failureReason)
	}

	// Phase 2: Implementation
	implementation, failureReason := g.executeImplementation(runner, contextResult)
	if failureReason != nil {
		return g.phaseFailureResult(failureReason)
	}

	// Success
	return g.successResult(implementation)
}

// createClient creates a new LLM client for this target
func (g *TargetCoder) createClient() (*llm.Client, error) {
	return llm.NewClient(g.coder.clientConfig, g.coder.httpClient, g.logger)
}

// executeContextGathering executes the context gathering phase
func (g *TargetCoder) executeContextGathering(runner *phase.Runner) (map[string]interface{}, *parser.FailureReason) {
	stepCallback := func(step string) {
		g.eventCallback(phase.PhaseContextGathering, step)
	}
	return runner.ExecuteContextGathering(g.ctx, g.target.Target, g.target.FileContent, g.coder.config.Dest, stepCallback)
}

// executeImplementation executes the implementation phase
func (g *TargetCoder) executeImplementation(runner *phase.Runner, contextResult map[string]interface{}) (string, *parser.FailureReason) {
	stepCallback := func(step string) {
		g.eventCallback(phase.PhaseImplementation, step)
	}
	return runner.ExecuteImplementation(g.ctx, g.target.Target, g.target.FileContent, g.target.FileInfo, g.projectRoot, contextResult, stepCallback)
}

// successResult creates a successful generation result
func (g *TargetCoder) successResult(implementation string) *parser.GenerationResult {
	duration := time.Since(g.startTime).Round(time.Millisecond)
	g.logger.Info("Successfully generated implementation", "duration", duration)
	g.callbacks.Complete(g.index)

	return &parser.GenerationResult{
		Target:         g.target.Target,
		Success:        true,
		Implementation: implementation,
		Duration:       duration,
	}
}

// failureResult creates a failure result
func (g *TargetCoder) failureResult(phase, message, context string) *parser.GenerationResult {
	g.callbacks.Fail(g.index)
	return &parser.GenerationResult{
		Target:  g.target.Target,
		Success: false,
		FailureReason: &parser.FailureReason{
			Phase:   phase,
			Message: message,
			Context: context,
		},
		Duration: time.Since(g.startTime).Round(time.Millisecond),
	}
}

// phaseFailureResult creates a failure result from a phase error
func (g *TargetCoder) phaseFailureResult(failureReason *parser.FailureReason) *parser.GenerationResult {
	g.callbacks.Fail(g.index)
	return &parser.GenerationResult{
		Target:        g.target.Target,
		Success:       false,
		FailureReason: failureReason,
		Duration:      time.Since(g.startTime).Round(time.Millisecond),
	}
}

// displayFailedTargetLogs displays logs only for failed targets in TUI mode
func (c *ParallelCoder) displayFailedTargetLogs(ctx context.Context, uiProgram *ui.Program) {
	// Only needed in TUI mode where logs are captured
	// In plain mode, logs are already displayed in real-time
	if !uiProgram.IsTUIEnabled() {
		return
	}

	failedTargets := uiProgram.GetFailedTargets()
	if len(failedTargets) == 0 {
		return
	}

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

// setupUI creates and configures the UI program with event handling
func (c *ParallelCoder) setupUI() (*ui.Program, chan phase.TargetEvent) {
	// Create TUI program for parallel execution
	// Use plain console output if --plain flag is set
	uiProgram := ui.NewProgramWithOptions(ui.ProgramOptions{
		Plain: c.config.Plain,
	})

	// Create event channel for phase updates
	eventCh := make(chan phase.TargetEvent, 100)

	// Start UI event processor
	go func() {
		for event := range eventCh {
			uiProgram.UpdatePhase(event.TargetIndex, event.Phase, event.Step)
		}
	}()

	return uiProgram, eventCh
}

// processTarget processes a single target with UI callbacks
func (c *ParallelCoder) processTarget(ctx context.Context, tc TargetContext, projectRoot string, index, total int, uiProgram *ui.Program, eventCh chan phase.TargetEvent) *parser.GenerationResult {
	// Register target with UI
	uiProgram.AddTarget(tc.Target.GetDisplayName(), index, total)

	// Create event callback for this target
	eventCallback := func(phaseName, step string) {
		eventCh <- phase.TargetEvent{
			TargetIndex: index,
			Phase:       phaseName,
			Step:        step,
			Time:        time.Now(),
		}
	}

	// Create target callbacks
	targetCallbacks := TargetCallbacks{
		SendLog: func(record slog.Record) {
			uiProgram.SendLog(record)
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

	// Create and run the target coder
	coder := NewTargetCoder(c, ctx, tc, projectRoot, index, total, targetCallbacks, eventCallback)
	return coder.Generate()
}

// waitForCompletion waits for generation and UI to complete
func (c *ParallelCoder) waitForCompletion(done chan struct{}, uiProgram *ui.Program) {
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
