package cmd

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/spf13/cobra"

	"github.com/rail44/glyph/internal/ai"
	"github.com/rail44/glyph/internal/generator"
	"github.com/rail44/glyph/internal/parser"
	"github.com/rail44/glyph/internal/prompt"
)

var (
	debugTiming  bool
	noStream     bool
	outputDir    string
	packageName  string
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
			fmt.Fprintf(os.Stderr, "Error: file %s does not exist\n", filePath)
			os.Exit(1)
		}

		// Make absolute path
		absPath, err := filepath.Abs(filePath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: failed to resolve path: %v\n", err)
			os.Exit(1)
		}

		// Run generation
		if err := runGeneration(absPath); err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
	},
}

func init() {
	rootCmd.AddCommand(generateCmd)

	generateCmd.Flags().BoolVar(&debugTiming, "debug-timing", false, "Show timing information for each step")
	generateCmd.Flags().BoolVar(&noStream, "no-stream", false, "Disable streaming output")
	generateCmd.Flags().StringVar(&outputDir, "output-dir", "./generated", "Directory for generated files")
	generateCmd.Flags().StringVar(&packageName, "package-name", "generated", "Package name for generated files")
}

func runGeneration(filePath string) error {
	fmt.Printf("Generating implementation for: %s\n", filePath)

	// Parse the file to find generation targets
	parseStart := time.Now()
	fmt.Println("Parsing file...")
	targets, err := parser.ParseFile(filePath)
	if err != nil {
		return fmt.Errorf("parse error: %w", err)
	}
	if debugTiming {
		fmt.Printf("  [Timing] Parsing took: %v\n", time.Since(parseStart))
	}

	if len(targets) == 0 {
		fmt.Println("No generation targets found (functions with // glyph comments)")
		return nil
	}

	fmt.Printf("Found %d generation targets\n", len(targets))

	// Create AI client
	clientStart := time.Now()
	config := &ai.Config{
		Model: GetModel(),
		Host:  GetHost(),
	}

	fmt.Printf("Creating AI client (model: %s)...\n", config.Model)
	aiClient, err := ai.NewClient(config)
	if err != nil {
		return fmt.Errorf("failed to create AI client: %w", err)
	}

	// Enable debug timing on AI client if requested
	aiClient.SetDebugTiming(debugTiming)

	if debugTiming {
		fmt.Printf("  [Timing] AI client creation took: %v\n", time.Since(clientStart))
	}

	// Check if model is available
	modelCheckStart := time.Now()
	ctx := context.Background()
	if err := aiClient.CheckModel(ctx); err != nil {
		fmt.Printf("Warning: Model check failed: %v\n", err)
		fmt.Println("Make sure the model is downloaded with: ollama pull", config.Model)
	}
	if debugTiming {
		fmt.Printf("  [Timing] Model check took: %v\n", time.Since(modelCheckStart))
	}

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
	return runSeparateGeneration(filePath, gen, promptBuilder, aiClient, targets, debugTiming, noStream)
}

// runSeparateGeneration handles generation with separate output files
func runSeparateGeneration(filePath string, gen *generator.Generator, promptBuilder *prompt.Builder, aiClient *ai.Client, targets []*parser.Target, debugTiming, noStream bool) error {
	totalStart := time.Now()

	// Parse file info for package details
	fileInfo, err := parser.ParseFileInfo(filePath)
	if err != nil {
		return fmt.Errorf("failed to parse file info: %w", err)
	}

	// Update generator config with source package
	if gen != nil {
		// Get the config from generator and update source package
		// This is a bit hacky, but we need to set the source package after we know it
		fmt.Printf("Source package: %s\n", fileInfo.PackageName)
	}

	if len(targets) == 0 {
		fmt.Println("No generation targets found (functions with // glyph comments)")
		return nil
	}

	fmt.Printf("Found %d generation targets, generating to separate file\n", len(targets))

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
		fmt.Printf("\n[%d/%d] Generating: %s\n", i+1, len(targets), target.Name)
		fmt.Printf("  Instruction: %s\n", target.Instruction)

		// Skip if no panic("not implemented")
		if !target.HasPanic {
			fmt.Println("  Skipping: no panic(\"not implemented\") found")
			continue
		}

		// Build prompt
		promptStart := time.Now()
		fullPrompt := promptBuilder.BuildForTarget(target, string(fileContent))
		if debugTiming {
			fmt.Printf("  [Timing] Prompt building took: %v\n", time.Since(promptStart))
			fmt.Printf("  [Timing] Prompt length: %d chars\n", len(fullPrompt))
		}

		// Generate implementation
		genStart := time.Now()
		var response string

		if noStream {
			// Non-streaming mode
			fmt.Println("  Generating implementation...")
			response, err = aiClient.Generate(ctx, fullPrompt)
			if err != nil {
				return fmt.Errorf("generation error for %s: %w", target.Name, err)
			}
		} else {
			// Streaming mode
			fmt.Print("  Generating implementation: ")

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
						fmt.Println() // New line after streaming
						goto streamDone
					}
					responseBuilder.WriteString(chunk)

					// Show progress dots instead of the actual code
					if firstOutput {
						firstOutput = false
					}
					charCount += len(chunk)
					// Print a dot for every 10 characters received
					for i := 0; i < len(chunk)/10; i++ {
						fmt.Print(".")
					}

				case err := <-errorCh:
					if err != nil {
						fmt.Println() // New line before error
						return fmt.Errorf("generation error for %s: %w", target.Name, err)
					}
				}
			}
		streamDone:
		}

		if debugTiming {
			fmt.Printf("  [Timing] AI generation took: %v\n", time.Since(genStart))
		}

		// Store implementation
		implementations[target.Name] = response

		fmt.Println("  ✓ Generated")
		if debugTiming {
			fmt.Printf("  [Timing] Total for %s: %v\n", target.Name, time.Since(targetStart))
		}
	}

	// Generate the output file with all implementations
	fmt.Printf("\nGenerating output file...")
	generateStart := time.Now()

	// Update source package in generator config
	// This is a workaround since we can't easily modify the config after creation
	// In a real implementation, we'd restructure this
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

	if debugTiming {
		fmt.Printf("  [Timing] File generation took: %v\n", time.Since(generateStart))
	}

	totalTime := time.Since(totalStart)
	fmt.Printf("\n✓ All implementations generated to %s!", filepath.Join(outputDir, filepath.Base(filePath)))
	if debugTiming {
		fmt.Printf("\n  [Timing] Total execution time: %v\n", totalTime)
	} else {
		fmt.Printf(" (took %v)\n", totalTime)
	}

	return nil
}
