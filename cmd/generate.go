package cmd

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
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
	statuses, err := detectAndSummarizeTargets(pkgDir, cfg.Dest)
	if err != nil {
		return err
	}

	// Check if any targets need generation
	needsGeneration := false
	for _, status := range statuses {
		if status.Status != detector.StatusCurrent {
			needsGeneration = true
			break
		}
	}

	if !needsGeneration {
		log.Info("all targets are up-to-date, nothing to generate")
		return nil
	}

	// Setup AI client and tools
	aiClient, promptBuilder, gen, err := setupAIClient(cfg, pkgDir)
	if err != nil {
		return err
	}

	ctx := context.Background()

	// Process targets grouped by file
	err = processTargetsByFile(ctx, statuses, aiClient, promptBuilder, gen, cfg.Dest)
	if err != nil {
		return err
	}

	log.Info("package generation complete")
	return nil
}

// detectAndSummarizeTargets detects targets and provides logging summary
func detectAndSummarizeTargets(pkgDir, destDir string) ([]*detector.TargetStatus, error) {
	log.Info("detecting targets in package", slog.String("package", pkgDir))
	statuses, err := detector.DetectPackageTargets(pkgDir, destDir)
	if err != nil {
		return nil, fmt.Errorf("failed to detect targets: %w", err)
	}

	// Summary of detection
	var ungenerated, outdated, current int
	for _, status := range statuses {
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
			log.Debug("up-to-date target",
				slog.String("function", status.Target.Name),
				slog.String("file", filepath.Base(status.Target.FilePath)))
		}
	}

	log.Info("detection summary",
		slog.Int("ungenerated", ungenerated),
		slog.Int("outdated", outdated),
		slog.Int("current", current),
		slog.Int("total", len(statuses)))

	// Return all statuses (including current ones with existing implementations)
	return statuses, nil
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

// processTargetsByFile groups targets by file and processes each file
func processTargetsByFile(ctx context.Context, statuses []*detector.TargetStatus, aiClient *ai.Client, promptBuilder *prompt.Builder, gen *generator.Generator, destDir string) error {
	// Group statuses by file
	statusesByFile := make(map[string][]*detector.TargetStatus)
	for _, status := range statuses {
		statusesByFile[status.Target.FilePath] = append(statusesByFile[status.Target.FilePath], status)
	}

	// Process each file
	for filePath, fileStatuses := range statusesByFile {
		// Count targets that need generation
		targetsNeedingGeneration := 0
		for _, status := range fileStatuses {
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
			slog.Int("total_targets", len(fileStatuses)))

		// Read file content
		content, err := os.ReadFile(filePath)
		if err != nil {
			log.Error("failed to read file",
				slog.String("file", filePath),
				slog.String("error", err.Error()))
			continue
		}

		// Parse file info
		fileInfo, err := parser.ParseFileInfo(filePath)
		if err != nil {
			log.Error("failed to parse file",
				slog.String("file", filePath),
				slog.String("error", err.Error()))
			continue
		}

		// Generate implementations only for targets that need it
		var targetsToGenerate []*parser.Target
		existingImplementations := make(map[string]string)

		for _, status := range fileStatuses {
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
				log.Info("generated file",
					slog.String("output", filepath.Join(destDir, filepath.Base(filePath))))
			}
		}
	}

	return nil
}

// generateImplementationsForTargets generates implementations for a list of targets from the same file
func generateImplementationsForTargets(ctx context.Context, targets []*parser.Target, fileContent string, aiClient *ai.Client, promptBuilder *prompt.Builder) (map[string]string, error) {
	implementations := make(map[string]string)

	for _, target := range targets {
		targetStart := time.Now()
		log.Info("starting generation",
			slog.String("function", target.Name))

		// Build prompt
		p, err := promptBuilder.BuildForTarget(target, fileContent)
		if err != nil {
			log.Error("failed to build prompt",
				slog.String("function", target.Name),
				slog.String("error", err.Error()))
			return nil, err
		}

		// Generate implementation
		log.Debug("attempting generation", slog.String("function", target.Name))
		implementation, genErr := aiClient.Generate(ctx, p)
		if genErr != nil {
			log.Debug("generation error",
				slog.String("function", target.Name),
				slog.String("error", genErr.Error()))
		} else {
			log.Debug("generation succeeded", slog.String("function", target.Name))
		}

		if genErr != nil {
			log.Error("failed to generate implementation",
				slog.String("function", target.Name),
				slog.String("error", genErr.Error()))
			continue
		}

		implementations[target.Name] = implementation
		log.Info("generated implementation",
			slog.String("function", target.Name),
			slog.Duration("duration", time.Since(targetStart)))
	}

	return implementations, nil
}
