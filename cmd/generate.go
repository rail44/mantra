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
	// Detect all targets and their status
	log.Info("detecting targets in package", slog.String("package", pkgDir))
	statuses, err := detector.DetectPackageTargets(pkgDir, cfg.Dest)
	if err != nil {
		return fmt.Errorf("failed to detect targets: %w", err)
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

	// Filter targets that need generation
	targetsToGenerate := detector.FilterTargetsToGenerate(statuses)
	if len(targetsToGenerate) == 0 {
		log.Info("all targets are up-to-date, nothing to generate")
		return nil
	}

	// Initialize AI client configuration
	clientConfig := &ai.ClientConfig{
		URL:     cfg.URL,
		APIKey:  cfg.APIKey,
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
		return fmt.Errorf("failed to create AI client: %w", err)
	}

	// Enable debug timing on AI client if requested
	aiClient.SetDebugTiming(log.IsDebugEnabled())

	// Log which provider we're using
	log.Info("using AI provider", 
		slog.String("provider", aiClient.GetProviderName()),
		slog.String("model", cfg.Model))

	// Check if model is available
	ctx := context.Background()
	if err := aiClient.CheckModel(ctx); err != nil {
		log.Warn("model check failed", 
			slog.String("error", err.Error()),
			slog.String("hint", "Check your API key, model availability, and base URL"))
	}

	promptBuilder := prompt.NewBuilder()
	// Note: tools are currently disabled in the simplified version
	
	gen := generator.New(&generator.Config{
		Dest:          cfg.Dest,
		PackageName:   cfg.GetPackageName(),
		SourcePackage: filepath.Base(pkgDir),
	})

	// Group targets by file
	targetsByFile := make(map[string][]*parser.Target)
	for _, target := range targetsToGenerate {
		targetsByFile[target.FilePath] = append(targetsByFile[target.FilePath], target)
	}

	// Process each file
	for filePath, targets := range targetsByFile {
		log.Info("processing file", 
			slog.String("file", filepath.Base(filePath)),
			slog.Int("targets", len(targets)))

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

		// Generate implementations
		implementations := make(map[string]string)
		for _, target := range targets {
			targetStart := time.Now()
			log.Info("generating implementation",
				slog.String("function", target.Name))

			// Build prompt
			p := promptBuilder.BuildForTarget(target, string(content))

			// Generate with AI (always streaming in simplified version)
			log.Info("generating implementation", slog.String("function", target.Name))
			outputCh, errorCh := aiClient.GenerateStream(ctx, p)
			var responseBuilder strings.Builder
			charCount := 0

			var implementation string
			var genErr error
			for {
				select {
				case chunk, ok := <-outputCh:
					if !ok {
						implementation = responseBuilder.String()
						goto streamDone
					}
					responseBuilder.WriteString(chunk)
					charCount += len(chunk)
					log.Trace("streaming progress", 
						slog.Int("chars_received", charCount),
						slog.String("function", target.Name))

				case err := <-errorCh:
					if err != nil {
						genErr = err
						goto streamDone
					}
				}
			}
			streamDone:

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

		// Generate file with all implementations
		if len(implementations) > 0 {
			if err := gen.GenerateFile(fileInfo, implementations); err != nil {
				log.Error("failed to generate file",
					slog.String("file", filePath),
					slog.String("error", err.Error()))
			} else {
				log.Info("generated file",
					slog.String("output", filepath.Join(cfg.Dest, filepath.Base(filePath))))
			}
		}
	}

	log.Info("package generation complete")
	return nil
}

