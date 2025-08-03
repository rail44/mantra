package cmd

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/spf13/cobra"
	"log/slog"

	"github.com/rail44/mantra/internal/ai"
	"github.com/rail44/mantra/internal/config"
	"github.com/rail44/mantra/internal/detector"
	"github.com/rail44/mantra/internal/generator"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/parser"
	"github.com/rail44/mantra/internal/prompt"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/setup"
)

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
	aiClient, promptBuilder, gen, err := setupAIClient(cfg, pkgDir)
	if err != nil {
		return err
	}

	ctx := context.Background()

	// Process all files
	err = processTargetsByFile(ctx, results, aiClient, promptBuilder, gen, cfg.Dest)
	if err != nil {
		return err
	}

	log.Info("package generation complete")
	return nil
}

// formatPath shortens a path for display by showing relative path from current directory
func formatPath(path string) string {
	cwd, err := os.Getwd()
	if err != nil {
		return filepath.Base(path)
	}
	relPath, err := filepath.Rel(cwd, path)
	if err != nil {
		return filepath.Base(path)
	}
	// If relative path is longer than absolute, just use basename
	if len(relPath) > len(path) {
		return filepath.Base(path)
	}
	return relPath
}

// detectAndSummarizeTargets detects targets and provides logging summary
func detectAndSummarizeTargets(pkgDir, destDir string) ([]*detector.FileDetectionResult, error) {
	log.Info("detecting targets in package", slog.String("package", formatPath(pkgDir)))
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
					slog.String("function", status.Target.Name),
					slog.String("file", filepath.Base(status.Target.FilePath)))
			case detector.StatusOutdated:
				outdated++
				log.Info("outdated target found",
					slog.String("function", status.Target.Name),
					slog.String("file", filepath.Base(status.Target.FilePath)),
					slog.String("old_checksum", status.ExistingChecksum),
					slog.String("new_checksum", status.CurrentChecksum))
			case detector.StatusCurrent:
				current++
				log.Debug(fmt.Sprintf("[SKIP] Up-to-date: %s.%s", filepath.Base(status.Target.FilePath), status.Target.Name))
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
func setupAIClient(cfg *config.Config, pkgDir string) (*ai.Client, *prompt.Builder, *generator.Generator, error) {
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

	// Generation config uses defaults
	generationConfig := ai.DefaultGenerationConfig()

	aiClient, err := ai.NewClient(clientConfig, generationConfig)
	if err != nil {
		return nil, nil, nil, fmt.Errorf("failed to create AI client: %w", err)
	}

	// Enable debug timing on AI client if requested
	aiClient.SetDebugTiming(log.IsDebugEnabled())

	// Log which provider we're using
	log.Info("using AI provider",
		slog.String("provider", aiClient.GetProviderName()),
		slog.String("model", cfg.Model))

	promptBuilder := prompt.NewBuilder()
	promptBuilder.SetUseTools(true) // Enable tools

	// Setup tools for AI
	toolRegistry := setup.InitializeRegistry(pkgDir)
	toolExecutor := tools.NewExecutor(toolRegistry)

	// Set tools on AI client
	aiClient.SetTools(toolRegistry.ListAvailable(), toolExecutor)

	gen := generator.New(&generator.Config{
		Dest:          cfg.Dest,
		PackageName:   cfg.GetPackageName(),
		SourcePackage: filepath.Base(pkgDir),
	})

	return aiClient, promptBuilder, gen, nil
}

// processTargetsByFile processes all files, generating implementations for targets and copying files without targets
func processTargetsByFile(ctx context.Context, results []*detector.FileDetectionResult, aiClient *ai.Client, promptBuilder *prompt.Builder, gen *generator.Generator, destDir string) error {
	// Process each file
	for _, result := range results {
		fileInfo := result.FileInfo
		filePath := fileInfo.FilePath

		// Handle files without mantra targets
		if len(result.Statuses) == 0 {
			// Simply copy the file with package name change
			if err := gen.GenerateFile(fileInfo, make(map[string]string)); err != nil {
				log.Error("failed to copy file without mantra targets",
					slog.String("file", filePath),
					slog.String("error", err.Error()))
			} else {
				log.Info(fmt.Sprintf("Copied: %s", filepath.Base(filePath)))
			}
			continue
		}

		// Count targets that need generation
		targetsNeedingGeneration := 0
		for _, status := range result.Statuses {
			if status.Status != detector.StatusCurrent {
				targetsNeedingGeneration++
			}
		}

		// Skip files where all targets are current
		if targetsNeedingGeneration == 0 {
			continue
		}

		log.Info("processing file",
			slog.String("file", filepath.Base(filePath)),
			slog.Int("targets_to_generate", targetsNeedingGeneration),
			slog.Int("total_targets", len(result.Statuses)))

		// Read file content
		content, err := os.ReadFile(filePath)
		if err != nil {
			log.Error("failed to read file",
				slog.String("file", filePath),
				slog.String("error", err.Error()))
			continue
		}

		// Generate implementations only for targets that need it
		var targetsToGenerate []*parser.Target
		existingImplementations := make(map[string]string)

		for _, status := range result.Statuses {
			if status.Status == detector.StatusCurrent {
				// Use existing implementation
				existingImplementations[status.Target.Name] = status.ExistingImpl
			} else {
				// Need to generate
				targetsToGenerate = append(targetsToGenerate, status.Target)
			}
		}

		// Generate new implementations
		newImplementations, err := generateImplementationsForTargets(ctx, targetsToGenerate, string(content), aiClient, promptBuilder)
		if err != nil {
			return fmt.Errorf("failed to generate implementations for file %s: %w", filePath, err)
		}

		// Merge existing and new implementations
		allImplementations := make(map[string]string)
		for name, impl := range existingImplementations {
			allImplementations[name] = impl
		}
		for name, impl := range newImplementations {
			allImplementations[name] = impl
		}

		// Generate file with all implementations
		if len(allImplementations) > 0 {
			if err := gen.GenerateFile(fileInfo, allImplementations); err != nil {
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

// generateImplementationsForTargets generates implementations for a list of targets from the same file
func generateImplementationsForTargets(ctx context.Context, targets []*parser.Target, fileContent string, aiClient *ai.Client, promptBuilder *prompt.Builder) (map[string]string, error) {
	implementations := make(map[string]string)

	for i, target := range targets {
		targetStart := time.Now()
		log.Info(fmt.Sprintf("[%d/%d] Generating %s...", i+1, len(targets), target.Name))

		// Build prompt
		p, err := promptBuilder.BuildForTarget(target, fileContent)
		if err != nil {
			log.Error("failed to build prompt",
				slog.String("function", target.Name),
				slog.String("error", err.Error()))
			return nil, err
		}

		// Generate implementation
		implementation, genErr := aiClient.Generate(ctx, p)

		if genErr != nil {
			log.Error(fmt.Sprintf("[%d/%d] Failed: %s - %s", i+1, len(targets), target.Name, genErr.Error()))
			continue
		}

		implementations[target.Name] = implementation
		log.Info(fmt.Sprintf("[%d/%d] Generated: %s (%s)", i+1, len(targets), target.Name, time.Since(targetStart).Round(time.Millisecond)))
	}

	return implementations, nil
}
