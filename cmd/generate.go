package cmd

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"log/slog"

	"github.com/spf13/cobra"

	"github.com/rail44/mantra/internal/ai"
	"github.com/rail44/mantra/internal/config"
	"github.com/rail44/mantra/internal/detector"
	"github.com/rail44/mantra/internal/generator"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/parser"
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

		// Set verbose flag in config
		cfg.Verbose = verbose

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
	// Detect targets
	results, err := detectTargets(pkgDir, cfg.Dest)
	if err != nil {
		return err
	}

	// Check if processing is needed
	if !needsProcessing(results) {
		log.Info("all files are up-to-date, nothing to generate")
		return nil
	}

	// Setup AI client and generator
	aiClient, gen, err := setupAIClient(cfg, pkgDir)
	if err != nil {
		return err
	}

	// Process all targets
	if err := processAllTargets(context.Background(), results, aiClient, gen, cfg); err != nil {
		return err
	}

	log.Info("package generation complete")
	return nil
}

// needsProcessing checks if any targets need generation or files need copying
func needsProcessing(results []*detector.FileDetectionResult) bool {
	for _, result := range results {
		// Files without targets need to be copied
		if len(result.Statuses) == 0 {
			return true
		}
		// Check if any target needs generation
		for _, status := range result.Statuses {
			if status.Status != detector.StatusCurrent {
				return true
			}
		}
	}
	return false
}

// detectTargets detects targets and provides logging summary
func detectTargets(pkgDir, destDir string) ([]*detector.FileDetectionResult, error) {
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

// processAllTargets processes all files, generating implementations for targets and copying files without targets
func processAllTargets(ctx context.Context, results []*detector.FileDetectionResult, aiClient *ai.Client, gen *generator.Generator, cfg *config.Config) error {
	// Collect targets and copy files without targets
	targets := collectTargets(results, gen)

	// Skip if no targets need generation
	if len(targets) == 0 {
		return nil
	}

	// Create and execute target executor
	executor := generator.NewTargetExecutor(aiClient, cfg)
	allResults, err := executor.ExecuteTargetsInParallel(ctx, targets)
	if err != nil {
		return fmt.Errorf("failed to generate implementations: %w", err)
	}

	// Write generated files
	return writeGeneratedFiles(results, allResults, gen)
}

// collectTargets collects targets that need generation and copies files without targets
func collectTargets(results []*detector.FileDetectionResult, gen *generator.Generator) []generator.TargetContext {
	var targets []generator.TargetContext

	for _, result := range results {
		fileInfo := result.FileInfo
		filePath := fileInfo.FilePath

		// Handle files without mantra targets
		if len(result.Statuses) == 0 {
			copyFileWithoutTargets(fileInfo, gen)
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
				targets = append(targets, generator.TargetContext{
					Target:      status.Target,
					FileContent: string(content),
					FileInfo:    result.FileInfo,
				})
			}
		}
	}

	return targets
}

// copyFileWithoutTargets copies a file that has no mantra targets
func copyFileWithoutTargets(fileInfo *parser.FileInfo, gen *generator.Generator) {
	if err := gen.GenerateFile(fileInfo, []*parser.GenerationResult{}); err != nil {
		log.Error("failed to copy file without mantra targets",
			slog.String("file", fileInfo.FilePath),
			slog.String("error", err.Error()))
	} else {
		log.Info(fmt.Sprintf("Copied: %s", filepath.Base(fileInfo.FilePath)))
	}
}

// writeGeneratedFiles writes all generated files with their results
func writeGeneratedFiles(results []*detector.FileDetectionResult, allResults []*parser.GenerationResult, gen *generator.Generator) error {
	// Group results by file
	fileResults := groupResultsByFile(allResults)

	for _, result := range results {
		if len(result.Statuses) == 0 {
			continue // Already handled
		}

		fileInfo := result.FileInfo
		filePath := fileInfo.FilePath

		// Collect all results for this file
		fileGenerationResults := collectFileGenerationResults(result, fileResults[filePath])

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

// groupResultsByFile groups generation results by their source file
func groupResultsByFile(allResults []*parser.GenerationResult) map[string][]*parser.GenerationResult {
	fileResults := make(map[string][]*parser.GenerationResult)
	for _, genResult := range allResults {
		filePath := genResult.Target.FilePath
		fileResults[filePath] = append(fileResults[filePath], genResult)
	}
	return fileResults
}

// collectFileGenerationResults collects all generation results for a file
func collectFileGenerationResults(detectionResult *detector.FileDetectionResult, generatedResults []*parser.GenerationResult) []*parser.GenerationResult {
	var fileGenerationResults []*parser.GenerationResult

	// Add newly generated results
	if generatedResults != nil {
		fileGenerationResults = append(fileGenerationResults, generatedResults...)
	}

	// Add existing implementations as successful results
	for _, status := range detectionResult.Statuses {
		if status.Status == detector.StatusCurrent {
			fileGenerationResults = append(fileGenerationResults, &parser.GenerationResult{
				Target:         status.Target,
				Success:        true,
				Implementation: status.ExistingImpl,
				Duration:       0, // No generation time for existing implementations
			})
		}
	}

	return fileGenerationResults
}
