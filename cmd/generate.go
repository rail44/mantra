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
	generateMode string
	debugTiming  bool
	noStream     bool
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
	
	generateCmd.Flags().StringVar(&generateMode, "mode", "generic", "Generation mode (spanner, generic, etc.)")
	generateCmd.Flags().BoolVar(&debugTiming, "debug-timing", false, "Show timing information for each step")
	generateCmd.Flags().BoolVar(&noStream, "no-stream", false, "Disable streaming output")
}

func runGeneration(filePath string) error {
	totalStart := time.Now()
	
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
	
	// Read file content for context
	readStart := time.Now()
	fileContent, err := os.ReadFile(filePath)
	if err != nil {
		return fmt.Errorf("failed to read file: %w", err)
	}
	if debugTiming {
		fmt.Printf("  [Timing] File read took: %v\n", time.Since(readStart))
	}
	
	// Use the mode from flag
	mode := generateMode
	
	// Create AI client
	clientStart := time.Now()
	config := &ai.Config{
		Model: GetModel(),
		Host:  GetHost(),
		Mode:  mode,
	}
	
	fmt.Printf("Creating AI client (model: %s, mode: %s)...\n", config.Model, config.Mode)
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
	
	// Create generator
	gen := generator.New()
	
	// Create prompt builder
	promptBuilder := prompt.NewBuilder(mode)
	
	// Process each target
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
		
		// Apply the generated code
		applyStart := time.Now()
		fmt.Println("  Applying generated code...")
		err = gen.GenerateForTarget(target, response)
		if err != nil {
			return fmt.Errorf("failed to apply generated code for %s: %w", target.Name, err)
		}
		if debugTiming {
			fmt.Printf("  [Timing] Code application took: %v\n", time.Since(applyStart))
		}
		
		fmt.Println("  ✓ Successfully generated")
		if debugTiming {
			fmt.Printf("  [Timing] Total for %s: %v\n", target.Name, time.Since(targetStart))
		}
		
		// Reload file content for next target (since file was modified)
		fileContent, err = os.ReadFile(filePath)
		if err != nil {
			return fmt.Errorf("failed to reload file: %w", err)
		}
	}
	
	totalTime := time.Since(totalStart)
	fmt.Printf("\n✓ All implementations generated successfully!")
	if debugTiming {
		fmt.Printf("\n  [Timing] Total execution time: %v\n", totalTime)
	} else {
		fmt.Printf(" (took %v)\n", totalTime)
	}
	return nil
}