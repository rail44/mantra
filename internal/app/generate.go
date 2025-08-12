package app

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"log/slog"

	"github.com/rail44/mantra/internal/codegen"
	"github.com/rail44/mantra/internal/coder"
	"github.com/rail44/mantra/internal/config"
	"github.com/rail44/mantra/internal/detector"
	"github.com/rail44/mantra/internal/llm"
	"github.com/rail44/mantra/internal/parser"
)

// GenerateApp handles the generate command logic
type GenerateApp struct {
	logger *slog.Logger
}

// NewGenerateApp creates a new generate app
func NewGenerateApp() *GenerateApp {
	return &GenerateApp{
		logger: slog.Default(),
	}
}

// Run executes the generate command
func (a *GenerateApp) Run(ctx context.Context, pkgDir string, cfg *config.Config) error {
	// Detect targets
	results, err := a.detectTargets(pkgDir, cfg.Dest)
	if err != nil {
		return err
	}

	// Check if processing is needed
	if !a.needsProcessing(results) {
		a.logger.Info("all files are up-to-date, nothing to generate")
		return nil
	}

	// Setup AI client configuration and generator
	clientConfig, gen, err := a.setupAIClient(cfg, pkgDir)
	if err != nil {
		return err
	}

	// Process all targets
	if err := a.processAllTargets(ctx, results, clientConfig, gen, cfg); err != nil {
		return err
	}

	a.logger.Info("package generation complete")
	return nil
}

// needsProcessing checks if any targets need generation or files need copying
func (a *GenerateApp) needsProcessing(results []*detector.FileDetectionResult) bool {
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
func (a *GenerateApp) detectTargets(pkgDir, destDir string) ([]*detector.FileDetectionResult, error) {
	a.logger.Info("detecting targets in package", slog.String("package", filepath.Base(pkgDir)))
	results, err := detector.DetectPackageTargets(pkgDir, destDir)
	if err != nil {
		return nil, fmt.Errorf("failed to detect targets: %w", err)
	}

	// Summary of detection
	var ungenerated, outdated, current, filesWithoutTargets int
	for _, result := range results {
		if len(result.Statuses) == 0 {
			filesWithoutTargets++
			a.logger.Debug(fmt.Sprintf("[FILE] No targets: %s", filepath.Base(result.FileInfo.FilePath)))
			continue
		}

		for _, status := range result.Statuses {
			switch status.Status {
			case detector.StatusUngenerated:
				ungenerated++
				a.logger.Info("new target found",
					slog.String("function", status.Target.GetDisplayName()),
					slog.String("file", filepath.Base(status.Target.FilePath)))
			case detector.StatusOutdated:
				outdated++
				a.logger.Info("outdated target found",
					slog.String("function", status.Target.GetDisplayName()),
					slog.String("file", filepath.Base(status.Target.FilePath)),
					slog.String("old_checksum", status.ExistingChecksum),
					slog.String("new_checksum", status.CurrentChecksum))
			case detector.StatusCurrent:
				current++
				a.logger.Debug(fmt.Sprintf("[SKIP] Up-to-date: %s.%s", filepath.Base(status.Target.FilePath), status.Target.GetDisplayName()))
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

	a.logger.Info(summary)

	// Return all results (including files without targets)
	return results, nil
}

// setupAIClient initializes AI client configuration and code generator
func (a *GenerateApp) setupAIClient(cfg *config.Config, pkgDir string) (*llm.ClientConfig, *codegen.Generator, error) {
	// Initialize AI client configuration
	clientConfig := &llm.ClientConfig{
		URL:     cfg.URL,
		APIKey:  cfg.GetAPIKey(),
		Model:   cfg.Model,
		Timeout: 5 * time.Minute,
	}

	// Set OpenRouter providers if configured
	if cfg.OpenRouter != nil && len(cfg.OpenRouter.Providers) > 0 {
		clientConfig.Provider = cfg.OpenRouter.Providers
	}

	// Log which provider we're using
	a.logger.Info("using AI provider",
		slog.String("url", cfg.URL),
		slog.String("model", cfg.Model))

	gen := codegen.New(&codegen.Config{
		Dest:          cfg.Dest,
		PackageName:   cfg.GetPackageName(),
		SourcePackage: filepath.Base(pkgDir),
	})

	return clientConfig, gen, nil
}

// processAllTargets processes all files, generating implementations for targets and copying files without targets
func (a *GenerateApp) processAllTargets(ctx context.Context, results []*detector.FileDetectionResult, clientConfig *llm.ClientConfig, gen *codegen.Generator, cfg *config.Config) error {
	// Prepare stub files for all targets before generation
	if err := a.prepareStubFiles(results, gen); err != nil {
		return fmt.Errorf("failed to prepare stub files: %w", err)
	}

	// Collect targets and copy files without targets
	targets := a.collectTargets(results, gen)

	// Skip if no targets need generation
	if len(targets) == 0 {
		return nil
	}

	// Create and execute target executor
	// Now PackageLoader will see the prepared files with correct structure
	parallelCoder := coder.NewParallelCoder(clientConfig, cfg)
	allResults, err := parallelCoder.ExecuteTargets(ctx, targets)
	if err != nil {
		return fmt.Errorf("failed to generate implementations: %w", err)
	}

	// Write generated files
	return a.writeGeneratedFiles(results, allResults, gen)
}

// prepareStubFiles prepares stub files for all targets before generation
func (a *GenerateApp) prepareStubFiles(results []*detector.FileDetectionResult, gen *codegen.Generator) error {
	for _, result := range results {
		fileInfo := result.FileInfo

		// Skip files without mantra targets
		if len(result.Statuses) == 0 {
			continue
		}

		// Collect targets that need generation for this file
		targetsToGenerate := make(map[string]bool)
		for _, status := range result.Statuses {
			if status.Status != detector.StatusCurrent {
				targetsToGenerate[status.Target.GetDisplayName()] = true
			}
		}

		// If there are targets to generate, prepare the stub file
		if len(targetsToGenerate) > 0 {
			if err := gen.PrepareTargetStubs(fileInfo, targetsToGenerate); err != nil {
				a.logger.Error("failed to prepare stub file",
					slog.String("file", fileInfo.FilePath),
					slog.String("error", err.Error()))
				return err
			}
			a.logger.Debug("prepared stub file",
				slog.String("file", fileInfo.FilePath),
				slog.Int("targets_to_generate", len(targetsToGenerate)))
		}
	}

	return nil
}

// collectTargets collects targets that need generation and copies files without targets
func (a *GenerateApp) collectTargets(results []*detector.FileDetectionResult, gen *codegen.Generator) []coder.TargetContext {
	var targets []coder.TargetContext

	for _, result := range results {
		fileInfo := result.FileInfo
		filePath := fileInfo.FilePath

		// Handle files without mantra targets
		if len(result.Statuses) == 0 {
			a.copyFileWithoutTargets(fileInfo, gen)
			continue
		}

		// Read file content once
		content, err := os.ReadFile(filePath)
		if err != nil {
			a.logger.Error("failed to read file",
				slog.String("file", filePath),
				slog.String("error", err.Error()))
			continue
		}

		// Collect targets that need generation
		for _, status := range result.Statuses {
			if status.Status != detector.StatusCurrent {
				targets = append(targets, coder.TargetContext{
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
func (a *GenerateApp) copyFileWithoutTargets(fileInfo *parser.FileInfo, gen *codegen.Generator) {
	if err := gen.GenerateFile(fileInfo, []*parser.GenerationResult{}); err != nil {
		a.logger.Error("failed to copy file without mantra targets",
			slog.String("file", fileInfo.FilePath),
			slog.String("error", err.Error()))
	} else {
		a.logger.Info(fmt.Sprintf("Copied: %s", filepath.Base(fileInfo.FilePath)))
	}
}

// writeGeneratedFiles writes all generated files with their results
func (a *GenerateApp) writeGeneratedFiles(results []*detector.FileDetectionResult, allResults []*parser.GenerationResult, gen *codegen.Generator) error {
	// Group results by file
	fileResults := a.groupResultsByFile(allResults)

	for _, result := range results {
		if len(result.Statuses) == 0 {
			continue // Already handled
		}

		fileInfo := result.FileInfo
		filePath := fileInfo.FilePath

		// Collect all results for this file
		fileGenerationResults := a.collectFileGenerationResults(result, fileResults[filePath])

		// Generate file with all results
		if len(fileGenerationResults) > 0 {
			if err := gen.GenerateFile(fileInfo, fileGenerationResults); err != nil {
				a.logger.Error("failed to generate file",
					slog.String("file", filePath),
					slog.String("error", err.Error()))
			} else {
				a.logger.Info(fmt.Sprintf("Generated: %s", filepath.Base(filePath)))
			}
		}
	}

	return nil
}

// groupResultsByFile groups generation results by their source file
func (a *GenerateApp) groupResultsByFile(allResults []*parser.GenerationResult) map[string][]*parser.GenerationResult {
	fileResults := make(map[string][]*parser.GenerationResult)
	for _, genResult := range allResults {
		filePath := genResult.Target.FilePath
		fileResults[filePath] = append(fileResults[filePath], genResult)
	}
	return fileResults
}

// collectFileGenerationResults collects all generation results for a file
func (a *GenerateApp) collectFileGenerationResults(detectionResult *detector.FileDetectionResult, generatedResults []*parser.GenerationResult) []*parser.GenerationResult {
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
