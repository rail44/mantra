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

// formatContextResultAsMarkdown converts the context gathering result from JSON to readable Markdown
func formatContextResultAsMarkdown(contextResult map[string]interface{}) string {
	if contextResult == nil {
		return ""
	}

	var formatted strings.Builder

	// Format types section
	if types, ok := contextResult["types"].([]interface{}); ok && len(types) > 0 {
		formatted.WriteString("### Discovered Types\n\n")
		for _, t := range types {
			if typeMap, ok := t.(map[string]interface{}); ok {
				if name, ok := typeMap["name"].(string); ok {
					formatted.WriteString(fmt.Sprintf("#### %s\n", name))
				}
				if definition, ok := typeMap["definition"].(string); ok {
					formatted.WriteString(fmt.Sprintf("```go\n%s\n```\n", definition))
				}
				if methods, ok := typeMap["methods"].([]interface{}); ok && len(methods) > 0 {
					formatted.WriteString("**Methods:**\n")
					for _, method := range methods {
						if methodStr, ok := method.(string); ok {
							formatted.WriteString(fmt.Sprintf("- %s\n", methodStr))
						}
					}
				}
				formatted.WriteString("\n")
			}
		}
	}

	// Format functions section
	if functions, ok := contextResult["functions"].([]interface{}); ok && len(functions) > 0 {
		formatted.WriteString("### Discovered Functions\n\n")
		for _, f := range functions {
			if funcMap, ok := f.(map[string]interface{}); ok {
				if name, ok := funcMap["name"].(string); ok {
					formatted.WriteString(fmt.Sprintf("#### %s\n", name))
				}
				if signature, ok := funcMap["signature"].(string); ok {
					formatted.WriteString(fmt.Sprintf("```go\n%s\n```\n", signature))
				}
				if implementation, ok := funcMap["implementation"].(string); ok && implementation != "" {
					formatted.WriteString("**Implementation:**\n")
					formatted.WriteString(fmt.Sprintf("```go\n%s\n```\n", implementation))
				}
				formatted.WriteString("\n")
			}
		}
	}

	// Format constants section
	if constants, ok := contextResult["constants"].([]interface{}); ok && len(constants) > 0 {
		formatted.WriteString("### Discovered Constants/Variables\n\n")
		for _, c := range constants {
			if constMap, ok := c.(map[string]interface{}); ok {
				if name, ok := constMap["name"].(string); ok {
					formatted.WriteString(fmt.Sprintf("- **%s**", name))
					if typeStr, ok := constMap["type"].(string); ok && typeStr != "" {
						formatted.WriteString(fmt.Sprintf(" (`%s`)", typeStr))
					}
					if value, ok := constMap["value"].(string); ok && value != "" {
						formatted.WriteString(fmt.Sprintf(" = `%s`", value))
					}
					formatted.WriteString("\n")
				}
			}
		}
		formatted.WriteString("\n")
	}

	return formatted.String()
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
	contextPhase.Reset() // Ensure clean state
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
	_, err = aiClient.Generate(ctx, initialPrompt)
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

	// Get result from the phase
	var contextResult map[string]interface{}
	var contextError *parser.FailureReason
	if phaseResult, completed := contextPhase.GetResult(); completed {
		// Check if it's an error result
		if resultMap, ok := phaseResult.(map[string]interface{}); ok {
			if success, hasSuccess := resultMap["success"].(bool); hasSuccess && !success {
				// Extract error information
				if errorField, hasError := resultMap["error"].(map[string]interface{}); hasError {
					message := ""
					details := ""
					if msg, ok := errorField["message"].(string); ok {
						message = msg
					}
					if det, ok := errorField["details"].(string); ok {
						details = det
					}
					contextError = &parser.FailureReason{
						Phase:   "context_gathering",
						Message: message,
						Context: details,
					}
				}
			} else if success {
				// Store the successful result directly
				contextResult = resultMap
			}
		}
	} else {
		// Fallback: no result from phase (shouldn't happen with result tool)
		logger.Warn("Context gathering phase did not complete with result tool")
		contextError = &parser.FailureReason{
			Phase:   "context_gathering",
			Message: "Phase did not complete properly",
			Context: "The result() tool was not called",
		}
	}

	// Check if we have an error from the result tool
	if contextError != nil {
		logger.Error("Context gathering failed",
			"phase", contextError.Phase,
			"message", contextError.Message,
			"context", contextError.Context,
			"target", target.Name)
		uiProgram.Fail(targetNum)
		return &parser.GenerationResult{
			Target:        target,
			Success:       false,
			FailureReason: contextError,
			Duration:      time.Since(targetStart),
		}
	}

	if contextResult != nil {
		// Convert to JSON for logging
		if resultJSON, err := json.Marshal(contextResult); err == nil {
			logger.Debug("Context gathering result", "length", len(resultJSON))
			logger.Trace("Context gathering output", "content", string(resultJSON))
		}
	}

	// Phase 2: Implementation
	logger.Info("Generating implementation...")
	uiProgram.UpdatePhase(targetNum, "Implementation", "Preparing")
	implPhase := phase.NewImplementationPhase(0.2, projectRoot, logger)
	implPhase.Reset() // Ensure clean state

	// Create tool context for static analysis
	toolContext := tools.NewContext(fileInfo, target, projectRoot)
	configureAIClientForPhase(aiClient, implPhase, logger, toolContext)

	// Build implementation prompt with context from phase 1
	// Convert contextResult to Markdown format for better AI understanding
	contextResultMarkdown := formatContextResultAsMarkdown(contextResult)
	implPromptBuilder := implPhase.GetPromptBuilderWithContext(contextResultMarkdown)
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
	_, err = aiClient.Generate(ctx, implPrompt)
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

	// Get result from the phase
	var implementation string
	var implError *parser.FailureReason
	if phaseResult, completed := implPhase.GetResult(); completed {
		// Check if it's a map with success/error structure
		if resultMap, ok := phaseResult.(map[string]interface{}); ok {
			if success, hasSuccess := resultMap["success"].(bool); hasSuccess {
				if !success {
					// Extract error information
					if errorField, hasError := resultMap["error"].(map[string]interface{}); hasError {
						message := ""
						details := ""
						if msg, ok := errorField["message"].(string); ok {
							message = msg
						}
						if det, ok := errorField["details"].(string); ok {
							details = det
						}
						implError = &parser.FailureReason{
							Phase:   "implementation",
							Message: message,
							Context: details,
						}
					}
				} else {
					// Extract the code for successful result
					if code, hasCode := resultMap["code"].(string); hasCode {
						implementation = code
					} else {
						implError = &parser.FailureReason{
							Phase:   "implementation",
							Message: "Missing code field in successful result",
							Context: "The result() tool was called with success=true but no code was provided",
						}
					}
				}
			} else {
				implError = &parser.FailureReason{
					Phase:   "implementation",
					Message: "Invalid result structure",
					Context: "The result() tool response is missing the success field",
				}
			}
		} else {
			logger.Error("Unexpected result type from implementation phase", "type", fmt.Sprintf("%T", phaseResult))
			implError = &parser.FailureReason{
				Phase:   "implementation",
				Message: "Invalid result type from implementation phase",
				Context: fmt.Sprintf("Expected map, got %T", phaseResult),
			}
		}
	} else {
		// Fallback: no result from phase (shouldn't happen with result tool)
		logger.Error("Implementation phase did not complete with result tool")
		implError = &parser.FailureReason{
			Phase:   "implementation",
			Message: "Implementation phase did not complete properly",
			Context: "The result() tool was not called",
		}
	}

	// Check if we have an error from the result tool
	if implError != nil {
		logger.Error("Implementation failed",
			"phase", implError.Phase,
			"message", implError.Message,
			"context", implError.Context,
			"target", target.Name)
		uiProgram.Fail(targetNum)
		return &parser.GenerationResult{
			Target:        target,
			Success:       false,
			FailureReason: implError,
			Duration:      time.Since(targetStart),
		}
	}

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
