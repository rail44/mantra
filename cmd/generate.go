package cmd

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"log/slog"

	"github.com/spf13/cobra"
	"golang.org/x/sync/errgroup"

	"github.com/rail44/mantra/internal/ai"
	"github.com/rail44/mantra/internal/config"
	"github.com/rail44/mantra/internal/detector"
	"github.com/rail44/mantra/internal/generator"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/parser"
	"github.com/rail44/mantra/internal/phase"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/ui"
)

var verbose bool

var generateCmd = &cobra.Command{
	Use:   "generate [package-dir]",
	Short: "Generate implementations for all pending targets in a package",
	Long: `Generate implementations for all mantra targets in a package that are either:
- Not yet generated (new targets)
- Outdated (declaration or instruction changed)

The command looks for functions marked with // mantra comments and generates
their implementations based on the natural language instructions provided.`,
	Args: cobra.MaximumNArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		// Get package directory (default to current directory)
		pkgDir := "."
		if len(args) > 0 {
			pkgDir = args[0]
		}

		// Load configuration
		cfg, err := config.Load(pkgDir)
		if err != nil {
			log.Error("failed to load configuration", slog.String("error", err.Error()))
			os.Exit(1)
		}

		// Set up logging
		logLevel := cfg.LogLevel
		if logLevel == "" {
			logLevel = "info"
		}
		level, err := log.ParseLevel(logLevel)
		if err != nil {
			log.Error("invalid log level", slog.String("level", logLevel))
			os.Exit(1)
		}
		if err := log.SetLevel(level); err != nil {
			log.Error("failed to set log level", slog.String("error", err.Error()))
			os.Exit(1)
		}

		// Ensure absolute path
		absPkgDir, err := filepath.Abs(pkgDir)
		if err != nil {
			log.Error("failed to get absolute path", slog.String("error", err.Error()))
			os.Exit(1)
		}

		// Run generation for package
		if err := runPackageGeneration(absPkgDir, cfg); err != nil {
			log.Error("generation failed", slog.String("error", err.Error()))
			os.Exit(1)
		}
	},
}

func init() {
	generateCmd.Flags().BoolVarP(&verbose, "verbose", "v", false, "Show detailed logs for all targets")
	rootCmd.AddCommand(generateCmd)
}

func runPackageGeneration(pkgDir string, cfg *config.Config) error {
	// Detect targets and check if generation is needed
	results, err := detectAndSummarizeTargets(pkgDir, cfg.Dest)
	if err != nil {
		return err
	}

	// Check if any targets need generation or any files need copying
	needsProcessing := false
	for _, result := range results {
		// Files without targets need to be copied
		if len(result.Statuses) == 0 {
			needsProcessing = true
			break
		}
		// Check if any target needs generation
		for _, status := range result.Statuses {
			if status.Status != detector.StatusCurrent {
				needsProcessing = true
				break
			}
		}
		if needsProcessing {
			break
		}
	}

	if !needsProcessing {
		log.Info("all files are up-to-date, nothing to generate")
		return nil
	}

	// Setup AI client and tools
	aiClient, gen, err := setupAIClient(cfg, pkgDir)
	if err != nil {
		return err
	}

	ctx := context.Background()

	// Process all files
	err = processTargetsByFile(ctx, results, aiClient, gen, cfg.Dest, cfg)
	if err != nil {
		return err
	}

	log.Info("package generation complete")
	return nil
}

// detectAndSummarizeTargets detects targets and provides logging summary
func detectAndSummarizeTargets(pkgDir, destDir string) ([]*detector.FileDetectionResult, error) {
	log.Info("detecting targets in package", slog.String("package", filepath.Base(pkgDir)))
	results, err := detector.DetectPackageTargets(pkgDir, destDir)
	if err != nil {
		return nil, fmt.Errorf("failed to detect targets: %w", err)
	}

	// Summary of detection
	var ungenerated, outdated, current, filesWithoutTargets int
	for _, result := range results {
		if len(result.Statuses) == 0 {
			filesWithoutTargets++
			log.Debug(fmt.Sprintf("[FILE] No targets: %s", filepath.Base(result.FileInfo.FilePath)))
			continue
		}

		for _, status := range result.Statuses {
			switch status.Status {
			case detector.StatusUngenerated:
				ungenerated++
				log.Info("new target found",
					slog.String("function", status.Target.GetDisplayName()),
					slog.String("file", filepath.Base(status.Target.FilePath)))
			case detector.StatusOutdated:
				outdated++
				log.Info("outdated target found",
					slog.String("function", status.Target.GetDisplayName()),
					slog.String("file", filepath.Base(status.Target.FilePath)),
					slog.String("old_checksum", status.ExistingChecksum),
					slog.String("new_checksum", status.CurrentChecksum))
			case detector.StatusCurrent:
				current++
				log.Debug(fmt.Sprintf("[SKIP] Up-to-date: %s.%s", filepath.Base(status.Target.FilePath), status.Target.GetDisplayName()))
			}
		}
	}

	// Build summary message
	var summaryParts []string
	if ungenerated > 0 {
		summaryParts = append(summaryParts, fmt.Sprintf("%d new", ungenerated))
	}
	if outdated > 0 {
		summaryParts = append(summaryParts, fmt.Sprintf("%d outdated", outdated))
	}
	if current > 0 {
		summaryParts = append(summaryParts, fmt.Sprintf("%d current", current))
	}
	if filesWithoutTargets > 0 {
		summaryParts = append(summaryParts, fmt.Sprintf("%d files without targets", filesWithoutTargets))
	}

	summary := fmt.Sprintf("Found: %s", strings.Join(summaryParts, ", "))
	if ungenerated == 0 && outdated == 0 && filesWithoutTargets == 0 {
		summary = "All targets up-to-date"
	}

	log.Info(summary)

	// Return all results (including files without targets)
	return results, nil
}

// setupAIClient initializes AI client, tools, and related components
func setupAIClient(cfg *config.Config, pkgDir string) (*ai.Client, *generator.Generator, error) {
	// Initialize AI client configuration
	clientConfig := &ai.ClientConfig{
		URL:     cfg.URL,
		APIKey:  cfg.GetAPIKey(),
		Model:   cfg.Model,
		Timeout: 5 * time.Minute,
	}

	// Set OpenRouter providers if configured
	if cfg.OpenRouter != nil && len(cfg.OpenRouter.Providers) > 0 {
		clientConfig.Provider = cfg.OpenRouter.Providers
	}

	aiClient, err := ai.NewClient(clientConfig, log.Default())
	if err != nil {
		return nil, nil, fmt.Errorf("failed to create AI client: %w", err)
	}

	// Log which provider we're using
	log.Info("using AI provider",
		slog.String("provider", aiClient.GetProviderName()),
		slog.String("model", cfg.Model))

	// Don't create tools here - they will be created per phase

	gen := generator.New(&generator.Config{
		Dest:          cfg.Dest,
		PackageName:   cfg.GetPackageName(),
		SourcePackage: filepath.Base(pkgDir),
	})

	return aiClient, gen, nil
}

// targetWithContext contains a target and its associated file context
type targetWithContext struct {
	target      *parser.Target
	fileContent string
	fileInfo    *parser.FileInfo
}

// processTargetsByFile processes all files, generating implementations for targets and copying files without targets
func processTargetsByFile(ctx context.Context, results []*detector.FileDetectionResult, aiClient *ai.Client, gen *generator.Generator, destDir string, cfg *config.Config) error {
	// Collect all targets that need generation across all files
	var allTargets []targetWithContext

	// First pass: collect targets and handle files without targets
	for _, result := range results {
		fileInfo := result.FileInfo
		filePath := fileInfo.FilePath

		// Handle files without mantra targets
		if len(result.Statuses) == 0 {
			// Simply copy the file with package name change
			if err := gen.GenerateFile(fileInfo, []*parser.GenerationResult{}); err != nil {
				log.Error("failed to copy file without mantra targets",
					slog.String("file", filePath),
					slog.String("error", err.Error()))
			} else {
				log.Info(fmt.Sprintf("Copied: %s", filepath.Base(filePath)))
			}
			continue
		}

		// Read file content once
		content, err := os.ReadFile(filePath)
		if err != nil {
			log.Error("failed to read file",
				slog.String("file", filePath),
				slog.String("error", err.Error()))
			continue
		}

		// Collect targets that need generation
		for _, status := range result.Statuses {
			if status.Status != detector.StatusCurrent {
				allTargets = append(allTargets, targetWithContext{
					target:      status.Target,
					fileContent: string(content),
					fileInfo:    result.FileInfo,
				})
			}
		}
	}

	// Skip if no targets need generation
	if len(allTargets) == 0 {
		return nil
	}

	// Generate all targets in parallel
	allResults, err := generateAllTargetsInParallel(ctx, allTargets, aiClient, cfg)
	if err != nil {
		return fmt.Errorf("failed to generate implementations: %w", err)
	}

	// Create implementation map for backward compatibility during transition
	implementations := make(map[string]string)
	for _, result := range allResults {
		if result.Success {
			implementations[result.Target.Name] = result.Implementation
		}
	}

	// Group results by file and generate output files
	fileResults := make(map[string][]*parser.GenerationResult)
	for _, genResult := range allResults {
		filePath := genResult.Target.FilePath
		fileResults[filePath] = append(fileResults[filePath], genResult)
	}

	for _, result := range results {
		if len(result.Statuses) == 0 {
			continue // Already handled
		}

		fileInfo := result.FileInfo
		filePath := fileInfo.FilePath

		// Collect GenerationResults for this file
		var fileGenerationResults []*parser.GenerationResult

		// Add results from generation
		if genResults, exists := fileResults[filePath]; exists {
			fileGenerationResults = append(fileGenerationResults, genResults...)
		}

		// Add existing implementations as successful results
		for _, status := range result.Statuses {
			if status.Status == detector.StatusCurrent {
				fileGenerationResults = append(fileGenerationResults, &parser.GenerationResult{
					Target:         status.Target,
					Success:        true,
					Implementation: status.ExistingImpl,
					Duration:       0, // No generation time for existing implementations
				})
			}
		}

		// Generate file with all results
		if len(fileGenerationResults) > 0 {
			if err := gen.GenerateFile(fileInfo, fileGenerationResults); err != nil {
				log.Error("failed to generate file",
					slog.String("file", filePath),
					slog.String("error", err.Error()))
			} else {
				log.Info(fmt.Sprintf("Generated: %s", filepath.Base(filePath)))
			}
		}
	}

	return nil
}

// configureAIClientForPhase configures the AI client with phase-specific settings
func configureAIClientForPhase(aiClient *ai.Client, p phase.Phase, logger log.Logger, toolContext *tools.Context) {
	aiClient.SetTemperature(p.GetTemperature())
	aiClient.SetSystemPrompt(p.GetSystemPrompt())

	// Get tools once and convert/create executor
	phaseTools := p.GetTools()
	aiTools := ai.ConvertToAITools(phaseTools)
	executor := tools.NewExecutor(phaseTools, logger)

	// Set context if provided
	if toolContext != nil {
		executor.SetContext(toolContext)
	}

	aiClient.SetTools(aiTools, executor)
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

// generateAllTargetsInParallel generates implementations for all targets across multiple files in parallel
func generateAllTargetsInParallel(ctx context.Context, targets []targetWithContext, aiClient *ai.Client, cfg *config.Config) ([]*parser.GenerationResult, error) {
	if len(targets) == 0 {
		return []*parser.GenerationResult{}, nil
	}

	// Get project root from the first target's file path
	projectRoot := findProjectRoot(filepath.Dir(targets[0].target.FilePath))

	// Create TUI program for parallel execution
	uiProgram := ui.NewProgram()

	// Thread-safe collections for collecting results
	var mu sync.Mutex
	implementations := make(map[string]string)
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
				result := generateImplementationForTargetWithUI(ctx, targetCtx.target, targetCtx.fileContent, targetCtx.fileInfo, aiClient, projectRoot, index, total, uiProgram, cfg)

				mu.Lock()
				allResults = append(allResults, result)
				if result.Success {
					implementations[targetCtx.target.Name] = result.Implementation
				}
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

	// Display detailed logs based on verbose flag
	if verbose {
		// Show logs for all targets when verbose is enabled
		allTargets := uiProgram.GetAllTargets()
		if len(allTargets) > 0 {
			fmt.Println()
			log.Info("Detailed logs for all targets")
			fmt.Println()

			for _, target := range allTargets {
				status := "Success"
				if target.Status == "failed" {
					status = "Failed"
				}
				log.Info(fmt.Sprintf("%s: %s", status, target.Name),
					slog.String("duration", target.EndTime.Sub(target.StartTime).Round(time.Millisecond).String()))

				logs := target.GetAllLogs()
				for _, logEntry := range logs {
					timestamp := logEntry.Timestamp.Format("15:04:05.000")
					fmt.Printf("  [%s] %s: %s\n", timestamp, logEntry.Level, logEntry.Message)
				}
				fmt.Println()
			}
		}
	} else {
		// Show logs only for failed targets when not verbose
		failedTargets := uiProgram.GetFailedTargets()
		if len(failedTargets) > 0 {
			fmt.Println()
			log.Error("Failed targets - Detailed logs")
			fmt.Println()

			for _, target := range failedTargets {
				log.Error(fmt.Sprintf("Failed: %s", target.Name),
					slog.String("duration", target.EndTime.Sub(target.StartTime).Round(time.Millisecond).String()))

				logs := target.GetAllLogs()
				for _, logEntry := range logs {
					timestamp := logEntry.Timestamp.Format("15:04:05.000")
					fmt.Printf("  [%s] %s: %s\n", timestamp, logEntry.Level, logEntry.Message)
				}
				fmt.Println()
			}

			log.Error("Total failures", slog.Int("count", len(failedTargets)))
		}
	}

	return allResults, nil
}

// parseAIResponse parses AI response and detects if it contains a failure indication
func parseAIResponse(response, phase string) (implementation string, failure *parser.FailureReason) {
	// Try to parse as JSON first
	trimmed := strings.TrimSpace(response)
	if strings.HasPrefix(trimmed, "{") {
		// Appears to be JSON response from structured output
		switch phase {
		case "context_gathering":
			var result struct {
				Imports   []interface{} `json:"imports"`
				Types     []interface{} `json:"types"`
				Functions []interface{} `json:"functions"`
				Summary   string        `json:"summary"`
			}
			if err := json.Unmarshal([]byte(trimmed), &result); err == nil {
				// Format the JSON result back into the expected markdown format
				var formatted strings.Builder

				if len(result.Types) > 0 {
					formatted.WriteString("### Types\n\n")
					for _, t := range result.Types {
						if typeMap, ok := t.(map[string]interface{}); ok {
							if name, ok := typeMap["name"].(string); ok {
								formatted.WriteString(fmt.Sprintf("#### %s\n\n", name))
								if def, ok := typeMap["definition"].(string); ok {
									formatted.WriteString(fmt.Sprintf("```go\n%s\n```\n\n", def))
								}
							}
						}
					}
				}

				if len(result.Functions) > 0 {
					formatted.WriteString("### Functions\n\n")
					for _, f := range result.Functions {
						if funcMap, ok := f.(map[string]interface{}); ok {
							if name, ok := funcMap["name"].(string); ok {
								formatted.WriteString(fmt.Sprintf("#### %s\n\n", name))
								if sig, ok := funcMap["signature"].(string); ok {
									formatted.WriteString(fmt.Sprintf("```go\n%s\n```\n\n", sig))
								}
							}
						}
					}
				}

				return formatted.String(), nil
			}

		case "implementation":
			var result struct {
				Code        string   `json:"code"`
				Explanation string   `json:"explanation"`
				Assumptions []string `json:"assumptions"`
			}
			if err := json.Unmarshal([]byte(trimmed), &result); err == nil {
				if result.Code == "" {
					return "", &parser.FailureReason{
						Phase:   phase,
						Message: "No code generated",
						Context: result.Explanation,
					}
				}
				return result.Code, nil
			}
		}
	}

	// Fall back to original text-based parsing
	// Check for explicit failure indication
	if strings.HasPrefix(trimmed, "GENERATION_FAILED:") {
		// Extract failure message
		message := strings.TrimPrefix(trimmed, "GENERATION_FAILED:")
		message = strings.TrimSpace(message)

		return "", &parser.FailureReason{
			Phase:   phase,
			Message: message,
			Context: "AI explicitly indicated generation cannot be completed",
		}
	}

	// Check for common failure patterns in responses
	lower := strings.ToLower(response)
	if strings.Contains(lower, "cannot implement") ||
		strings.Contains(lower, "unable to implement") ||
		strings.Contains(lower, "insufficient information") ||
		strings.Contains(lower, "not enough context") {
		return "", &parser.FailureReason{
			Phase:   phase,
			Message: "AI indicated implementation difficulties: " + response,
			Context: "AI response suggests implementation issues",
		}
	}

	return response, nil
}

// generateImplementationForTargetWithUI generates implementation with TUI logger
func generateImplementationForTargetWithUI(ctx context.Context, target *parser.Target, fileContent string, fileInfo *parser.FileInfo, baseAIClient *ai.Client, projectRoot string, targetNum, totalTargets int, uiProgram *ui.Program, cfg *config.Config) *parser.GenerationResult {
	targetStart := time.Now()

	// Create a target-specific logger with display name (includes receiver for methods)
	logger := uiProgram.CreateTargetLogger(target.GetDisplayName(), targetNum, totalTargets)

	// Create a new AI client with the target-specific logger
	// This avoids concurrent access issues with shared client
	aiClient, err := ai.NewClient(baseAIClient.GetConfig(), logger)
	if err != nil {
		logger.Error("Failed to create AI client", "error", err.Error())
		uiProgram.Fail(targetNum)
		return &parser.GenerationResult{
			Target:  target,
			Success: false,
			FailureReason: &parser.FailureReason{
				Phase:   "initialization",
				Message: "Failed to create AI client: " + err.Error(),
				Context: "Client configuration error",
			},
			Duration: time.Since(targetStart),
		}
	}

	logger.Info("Starting generation")
	uiProgram.UpdatePhase(targetNum, "Context Gathering", "Starting")

	// Phase 1: Context Gathering
	logger.Info("Analyzing codebase context...")
	packagePath := filepath.Dir(target.FilePath) // Get the package directory
	contextPhase := phase.NewContextGatheringPhase(0.6, packagePath, logger)
	// Context gathering doesn't need tool context
	configureAIClientForPhase(aiClient, contextPhase, logger, nil)

	// Build initial prompt
	contextPromptBuilder := contextPhase.GetPromptBuilder()
	initialPrompt, err := contextPromptBuilder.BuildForTarget(target, fileContent)
	if err != nil {
		logger.Error("Failed to build prompt", "error", err.Error())
		uiProgram.Fail(targetNum)
		return &parser.GenerationResult{
			Target:  target,
			Success: false,
			FailureReason: &parser.FailureReason{
				Phase:   "context_gathering",
				Message: "Failed to build context gathering prompt: " + err.Error(),
				Context: "Prompt construction error",
			},
			Duration: time.Since(targetStart),
		}
	}

	// Execute context gathering
	uiProgram.UpdatePhase(targetNum, "Context Gathering", "Analyzing codebase")
	contextResult, err := aiClient.Generate(ctx, initialPrompt)
	if err != nil {
		logger.Error("Context gathering failed", "error", err.Error())
		uiProgram.Fail(targetNum)
		return &parser.GenerationResult{
			Target:  target,
			Success: false,
			FailureReason: &parser.FailureReason{
				Phase:   "context_gathering",
				Message: "AI context gathering failed: " + err.Error(),
				Context: "May be due to insufficient codebase information or AI service issues",
			},
			Duration: time.Since(targetStart),
		}
	}

	// Parse context gathering response for failure indications
	parsedContext, contextFailure := parseAIResponse(contextResult, "context_gathering")
	if contextFailure != nil {
		logger.Error("Context gathering indicated failure",
			"phase", contextFailure.Phase,
			"message", contextFailure.Message,
			"context", contextFailure.Context,
			"target", target.Name)
		uiProgram.Fail(targetNum)
		return &parser.GenerationResult{
			Target:        target,
			Success:       false,
			FailureReason: contextFailure,
			Duration:      time.Since(targetStart),
		}
	}
	contextResult = parsedContext

	logger.Debug("Context gathering result", "length", len(contextResult))
	// Log the actual context content at trace level
	logger.Trace("Context gathering output", "content", contextResult)

	// Phase 2: Implementation
	logger.Info("Generating implementation...")
	uiProgram.UpdatePhase(targetNum, "Implementation", "Preparing")
	implPhase := phase.NewImplementationPhase(0.2, projectRoot, logger)

	// Create tool context for static analysis
	toolContext := tools.NewContext(fileInfo, target, projectRoot)
	configureAIClientForPhase(aiClient, implPhase, logger, toolContext)

	// Build implementation prompt with context from phase 1
	implPromptBuilder := implPhase.GetPromptBuilderWithContext(contextResult)
	implPrompt, err := implPromptBuilder.BuildForTarget(target, fileContent)
	if err != nil {
		logger.Error("Failed to build implementation prompt", "error", err.Error())
		uiProgram.Fail(targetNum)
		return &parser.GenerationResult{
			Target:  target,
			Success: false,
			FailureReason: &parser.FailureReason{
				Phase:   "implementation",
				Message: "Failed to build implementation prompt: " + err.Error(),
				Context: "Error occurred while incorporating context from phase 1",
			},
			Duration: time.Since(targetStart),
		}
	}

	// Generate implementation
	uiProgram.UpdatePhase(targetNum, "Implementation", "Generating code")
	implementation, err := aiClient.Generate(ctx, implPrompt)
	if err != nil {
		logger.Error("Implementation failed", "error", err.Error())
		uiProgram.Fail(targetNum)
		return &parser.GenerationResult{
			Target:  target,
			Success: false,
			FailureReason: &parser.FailureReason{
				Phase:   "implementation",
				Message: "AI implementation generation failed: " + err.Error(),
				Context: "May be due to complex requirements or AI service issues",
			},
			Duration: time.Since(targetStart),
		}
	}

	// Parse implementation response for failure indications
	parsedImplementation, implementationFailure := parseAIResponse(implementation, "implementation")
	if implementationFailure != nil {
		logger.Error("Implementation indicated failure",
			"phase", implementationFailure.Phase,
			"message", implementationFailure.Message,
			"context", implementationFailure.Context,
			"target", target.Name)
		uiProgram.Fail(targetNum)
		return &parser.GenerationResult{
			Target:        target,
			Success:       false,
			FailureReason: implementationFailure,
			Duration:      time.Since(targetStart),
		}
	}
	implementation = parsedImplementation

	duration := time.Since(targetStart).Round(time.Millisecond)
	logger.Info("Successfully generated implementation", "duration", duration)
	uiProgram.Complete(targetNum)

	return &parser.GenerationResult{
		Target:         target,
		Success:        true,
		Implementation: implementation,
		Duration:       duration,
	}
}
