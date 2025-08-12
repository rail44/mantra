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

	uiProgram := ui.NewProgramWithOptions(ui.ProgramOptions{
		Plain: c.config.Plain,
	})

	// Thread-safe collections for collecting results
	var mu sync.Mutex
	var allResults []*parser.GenerationResult

	// Start TUI in background
	tuiDone := make(chan error, 1)
	go func() {
		tuiDone <- uiProgram.Start()
	}()

	// Start generation in a separate goroutine
	genDone := make(chan struct{})
	go func() {
		defer close(genDone)
		
		// Use errgroup with limited concurrency (max 16)
		g, ctx := errgroup.WithContext(ctx)
		g.SetLimit(16)

		// Process each target in parallel
		for i, tc := range targets {
			index := i + 1
			targetCtx := tc

			g.Go(func() error {
				// Register target with UI
				uiProgram.AddTarget(targetCtx.Target.GetDisplayName(), index, len(targets))
				
				// Create and run the target coder
				coder := NewTargetCoder(c, ctx, targetCtx, projectRoot, index, len(targets), uiProgram)
				result := coder.Generate()
				
				mu.Lock()
				allResults = append(allResults, result)
				mu.Unlock()
				return nil
			})
		}

		// Wait for all goroutines to complete
		g.Wait()
	}()

	// Wait for generation to complete
	<-genDone
	
	// Stop the UI
	time.Sleep(100 * time.Millisecond) // Allow final render
	uiProgram.Quit()
	
	// Wait for UI to finish if it's enabled
	if uiProgram.IsTUIEnabled() {
		<-tuiDone
	}

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

// TargetCoder handles the code generation for a single target
type TargetCoder struct {
	coder       *ParallelCoder
	ctx         context.Context
	target      TargetContext
	projectRoot string
	index       int
	total       int
	uiProgram   *ui.Program
	logger      *slog.Logger
	startTime   time.Time
}

// NewTargetCoder creates a new target coder
func NewTargetCoder(coder *ParallelCoder, ctx context.Context, target TargetContext, projectRoot string, index, total int, uiProgram *ui.Program) *TargetCoder {
	// Create a callback handler with target attributes
	callbackHandler := log.NewCallbackHandler(
		func(record slog.Record) {
			uiProgram.SendLog(record)
		},
	).WithAttrs([]slog.Attr{
		slog.Int("targetIndex", index),
		slog.Int("totalTargets", total),
		slog.String("targetName", target.Target.GetDisplayName()),
	})

	return &TargetCoder{
		coder:       coder,
		ctx:         ctx,
		target:      target,
		projectRoot: projectRoot,
		index:       index,
		total:       total,
		uiProgram:   uiProgram,
		logger:      slog.New(callbackHandler),
		startTime:   time.Now(),
	}
}

// Generate executes the code generation process for the target
func (g *TargetCoder) Generate() *parser.GenerationResult {
	// Log generation start
	g.logger.Info("Starting generation")

	// Mark target as running
	g.markRunning()

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
func (g *TargetCoder) executeContextGathering(runner *phase.Runner) (map[string]any, *parser.FailureReason) {
	stepCallback := func(step string) {
		g.sendPhaseEvent(phase.PhaseContextGathering, step)
	}
	return runner.ExecuteContextGathering(g.ctx, g.target.Target, g.target.FileContent, g.coder.config.Dest, stepCallback)
}

// executeImplementation executes the implementation phase
func (g *TargetCoder) executeImplementation(runner *phase.Runner, contextResult map[string]any) (string, *parser.FailureReason) {
	stepCallback := func(step string) {
		g.sendPhaseEvent(phase.PhaseImplementation, step)
	}
	return runner.ExecuteImplementation(g.ctx, g.target.Target, g.target.FileContent, g.target.FileInfo, g.projectRoot, contextResult, stepCallback)
}

// successResult creates a successful generation result
func (g *TargetCoder) successResult(implementation string) *parser.GenerationResult {
	duration := time.Since(g.startTime).Round(time.Millisecond)
	g.logger.Info("Successfully generated implementation", "duration", duration)
	g.markComplete()

	return &parser.GenerationResult{
		Target:         g.target.Target,
		Success:        true,
		Implementation: implementation,
		Duration:       duration,
	}
}

// failureResult creates a failure result
func (g *TargetCoder) failureResult(phase, message, context string) *parser.GenerationResult {
	g.markFailed()
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
	g.markFailed()
	return &parser.GenerationResult{
		Target:        g.target.Target,
		Success:       false,
		FailureReason: failureReason,
		Duration:      time.Since(g.startTime).Round(time.Millisecond),
	}
}

// UI callback methods

// markRunning marks the target as running
func (g *TargetCoder) markRunning() {
	g.uiProgram.MarkAsRunning(g.index)
}

// markComplete marks the target as complete
func (g *TargetCoder) markComplete() {
	g.uiProgram.Complete(g.index)
}

// markFailed marks the target as failed
func (g *TargetCoder) markFailed() {
	g.uiProgram.Fail(g.index)
}

// sendPhaseEvent sends a phase event to the UI
func (g *TargetCoder) sendPhaseEvent(phaseName, step string) {
	g.uiProgram.UpdatePhase(g.index, phaseName, step)
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
