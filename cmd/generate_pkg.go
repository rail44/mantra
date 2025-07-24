package cmd

import (
	"log/slog"
	"os"
	"path/filepath"

	"github.com/rail44/mantra/internal/ai"
	"github.com/rail44/mantra/internal/detector"
	"github.com/rail44/mantra/internal/generator"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/parser"
	"github.com/rail44/mantra/internal/prompt"
	"github.com/spf13/cobra"
)

var generatePkgCmd = &cobra.Command{
	Use:   "generate-pkg [package-dir]",
	Short: "Generate implementations for all pending targets in a package",
	Long: `Generate implementations for all mantra targets in a package that are either:
- Not yet generated (new targets)
- Outdated (declaration or instruction changed)`,
	Args: cobra.MaximumNArgs(1),
	Run:  runGeneratePkg,
}

func init() {
	rootCmd.AddCommand(generatePkgCmd)
}

func runGeneratePkg(cmd *cobra.Command, args []string) {
	// Get package directory (default to current directory)
	pkgDir := "."
	if len(args) > 0 {
		pkgDir = args[0]
	}

	// Ensure absolute path
	absPkgDir, err := filepath.Abs(pkgDir)
	if err != nil {
		log.Error("failed to get absolute path", slog.String("error", err.Error()))
		os.Exit(1)
	}

	// Output directory
	outputDir := filepath.Join(absPkgDir, "generated")

	// Detect all targets and their status
	log.Info("detecting targets in package", slog.String("package", absPkgDir))
	statuses, err := detector.DetectPackageTargets(absPkgDir, outputDir)
	if err != nil {
		log.Error("failed to detect targets", slog.String("error", err.Error()))
		os.Exit(1)
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
		return
	}

	// Initialize components
	aiClient, err := ai.NewClient(&ai.Config{
		Host:  ollamaHost,
		Model: modelName,
	})
	if err != nil {
		log.Error("failed to create Ollama client", slog.String("error", err.Error()))
		os.Exit(1)
	}
	// aiClient doesn't have Close method in current implementation

	promptBuilder := prompt.NewBuilder()
	gen := generator.New(&generator.Config{
		OutputDir:     outputDir,
		PackageName:   "main", // TODO: make this configurable
		SourcePackage: filepath.Base(absPkgDir),
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
			log.Info("generating implementation",
				slog.String("function", target.Name))

			// Build prompt
			p := promptBuilder.BuildForTarget(target, string(content))

			// Generate with AI
			implementation, err := aiClient.Generate(cmd.Context(), p)
			if err != nil {
				log.Error("failed to generate implementation",
					slog.String("function", target.Name),
					slog.String("error", err.Error()))
				continue
			}

			implementations[target.Name] = implementation
			log.Info("generated implementation",
				slog.String("function", target.Name))
		}

		// Generate file with all implementations
		if len(implementations) > 0 {
			if err := gen.GenerateFile(fileInfo, implementations); err != nil {
				log.Error("failed to generate file",
					slog.String("file", filePath),
					slog.String("error", err.Error()))
			} else {
				log.Info("generated file",
					slog.String("output", filepath.Join(outputDir, filepath.Base(filePath))))
			}
		}
	}

	log.Info("package generation complete")
}