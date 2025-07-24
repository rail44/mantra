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

	"github.com/rail44/glyph/internal/ai"
	"github.com/rail44/glyph/internal/generator"
	"github.com/rail44/glyph/internal/log"
	"github.com/rail44/glyph/internal/parser"
	"github.com/rail44/glyph/internal/prompt"
)

var (
	noStream    bool
	outputDir   string
	packageName string
)

var generateCmd = &cobra.Command{
	Use:   "generate <file>",
	Short: "Generate implementation once without watching",
	Long: `Generate runs the AI generation process once on the specified Go file
and replaces panic("not implemented") with actual implementations.

The command looks for functions marked with // glyph comments and generates
their implementations based on the natural language instructions provided.`,
	Args: cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		filePath := args[0]

		// Verify file exists
		if _, err := os.Stat(filePath); os.IsNotExist(err) {
			log.Error("file does not exist", slog.String("file", filePath))
			os.Exit(1)
		}

		// Make absolute path
		absPath, err := filepath.Abs(filePath)
		if err != nil {
			log.Error("failed to resolve path", slog.String("error", err.Error()))
			os.Exit(1)
		}

		// Run generation
		if err := runGeneration(absPath); err != nil {
			log.Error("generation failed", slog.String("error", err.Error()))
			os.Exit(1)
		}
	},
}

func init() {
	rootCmd.AddCommand(generateCmd)

	generateCmd.Flags().BoolVar(&noStream, "no-stream", false, "Disable streaming output")
	generateCmd.Flags().StringVar(&outputDir, "output-dir", "./generated", "Directory for generated files")
	generateCmd.Flags().StringVar(&packageName, "package-name", "generated", "Package name for generated files")
}

func runGeneration(filePath string) error {
	log.Info("generating implementation", slog.String("file", filePath))

	// Parse the file to find generation targets
	parseStart := time.Now()
	log.Info("parsing file", slog.String("file", filepath.Base(filePath)))
	targets, err := parser.ParseFile(filePath)
	if err != nil {
		return fmt.Errorf("parse error: %w", err)
	}
	log.Debug("parsing completed", 
		slog.Duration("duration", time.Since(parseStart)),
		slog.Int("targets", len(targets)))

	if len(targets) == 0 {
		log.Info("no generation targets found (functions with // glyph comments)")
		return nil
	}

	log.Info("found generation targets", slog.Int("count", len(targets)))

	// Create AI client
	clientStart := time.Now()
	config := &ai.Config{
		Model: GetModel(),
		Host:  GetHost(),
	}
	log.Info("creating AI client", slog.String("model", config.Model))
	aiClient, err := ai.NewClient(config)
	if err != nil {
		return fmt.Errorf("failed to create AI client: %w", err)
	}

	// Enable debug timing on AI client if requested
	aiClient.SetDebugTiming(log.IsDebugEnabled())

	log.Debug("AI client created", slog.Duration("duration", time.Since(clientStart)))

	// Check if model is available
	modelCheckStart := time.Now()
	ctx := context.Background()
	if err := aiClient.CheckModel(ctx); err != nil {
		log.Warn("model check failed", 
			slog.String("error", err.Error()),
			slog.String("hint", fmt.Sprintf("Make sure the model is downloaded with: ollama pull %s", config.Model)))
	}
	log.Debug("model check completed", slog.Duration("duration", time.Since(modelCheckStart)))

	// Always use separate output approach
	genConfig := &generator.Config{
		OutputDir:     outputDir,
		PackageName:   packageName,
		SourcePackage: "", // Will be determined from file
	}
	gen := generator.New(genConfig)

	// Create prompt builder
	promptBuilder := prompt.NewBuilder()

	// Use separate generation approach
	return runSeparateGeneration(filePath, gen, promptBuilder, aiClient, targets, noStream)
}

// runSeparateGeneration handles generation with separate output files
func runSeparateGeneration(filePath string, gen *generator.Generator, promptBuilder *prompt.Builder, aiClient *ai.Client, targets []*parser.Target, noStream bool) error {
	totalStart := time.Now()

	// Parse file info for package details
	fileInfo, err := parser.ParseFileInfo(filePath)
	if err != nil {
		return fmt.Errorf("failed to parse file info: %w", err)
	}

	// Log source package information
	log.Info("source package", slog.String("package", fileInfo.PackageName))

	if len(targets) == 0 {
		log.Info("no generation targets found (functions with // glyph comments)")
		return nil
	}

	log.Info("generating to separate file", slog.Int("targets", len(targets)))

	// Read file content for context
	fileContent, err := os.ReadFile(filePath)
	if err != nil {
		return fmt.Errorf("failed to read file: %w", err)
	}

	// Generate implementations for all targets
	implementations := make(map[string]string)
	ctx := context.Background()

	for i, target := range targets {
		targetStart := time.Now()
		log.Info("generating target", 
			slog.Int("current", i+1),
			slog.Int("total", len(targets)),
			slog.String("function", target.Name),
			slog.String("instruction", target.Instruction))

		// Skip if no panic("not implemented")
		if !target.HasPanic {
			log.Info("skipping target - no panic found", slog.String("function", target.Name))
			continue
		}

		// Build prompt
		promptStart := time.Now()
		fullPrompt := promptBuilder.BuildForTarget(target, string(fileContent))
		log.Debug("prompt built",
			slog.Duration("duration", time.Since(promptStart)),
			slog.Int("length", len(fullPrompt)))

		// Generate implementation
		genStart := time.Now()
		var response string

		if noStream {
			// Non-streaming mode
			log.Info("generating implementation (non-streaming)", slog.String("function", target.Name))
			response, err = aiClient.Generate(ctx, fullPrompt)
			if err != nil {
				return fmt.Errorf("generation error for %s: %w", target.Name, err)
			}
		} else {
			// Streaming mode
			log.Info("generating implementation (streaming)", slog.String("function", target.Name))

			outputCh, errorCh := aiClient.GenerateStream(ctx, fullPrompt)
			var responseBuilder strings.Builder

			// Track if we've shown any output
			firstOutput := true
			charCount := 0

			// Process streaming output
			for {
				select {
				case chunk, ok := <-outputCh:
					if !ok {
						// Channel closed, we're done
						response = responseBuilder.String()
						goto streamDone
					}
					responseBuilder.WriteString(chunk)

					// Show progress dots instead of the actual code
					if firstOutput {
						firstOutput = false
					}
					charCount += len(chunk)
					// Log progress at trace level
					log.Trace("streaming progress", 
						slog.Int("chars_received", charCount),
						slog.String("function", target.Name))

				case err := <-errorCh:
					if err != nil {
						return fmt.Errorf("generation error for %s: %w", target.Name, err)
					}
				}
			}
		streamDone:
		}

		log.Debug("AI generation completed", 
			slog.Duration("duration", time.Since(genStart)),
			slog.String("function", target.Name))

		// Store implementation
		implementations[target.Name] = response

		log.Info("target generated successfully", slog.String("function", target.Name))
		log.Debug("target total time", 
			slog.String("function", target.Name),
			slog.Duration("duration", time.Since(targetStart)))
	}

	// Generate the output file with all implementations
	log.Info("generating output file")
	generateStart := time.Now()

	// Create new generator with correct source package
	genConfig := &generator.Config{
		OutputDir:     outputDir,
		PackageName:   packageName,
		SourcePackage: fileInfo.PackageName,
	}
	gen = generator.New(genConfig)

	err = gen.GenerateFile(fileInfo, implementations)
	if err != nil {
		return fmt.Errorf("failed to generate output file: %w", err)
	}

	log.Debug("file generation completed", slog.Duration("duration", time.Since(generateStart)))

	totalTime := time.Since(totalStart)
	outputPath := filepath.Join(outputDir, filepath.Base(filePath))
	log.Info("all implementations generated successfully", 
		slog.String("output", outputPath),
		slog.Duration("total_time", totalTime))

	return nil
}
