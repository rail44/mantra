package cmd

import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	"github.com/spf13/cobra"
	"github.com/spf13/viper"
	
	"github.com/rail44/glyph/internal/ai"
	"github.com/rail44/glyph/internal/generator"
	"github.com/rail44/glyph/internal/parser"
	"github.com/rail44/glyph/internal/prompt"
)

var generateCmd = &cobra.Command{
	Use:   "generate <file>",
	Short: "Generate implementation once without watching",
	Long: `Generate runs the AI generation process once on the specified declaration file
and creates the implementation file. This is useful for one-time generation or
integration with other tools.`,
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
}

func runGeneration(filePath string) error {
	fmt.Printf("Generating implementation for: %s\n", filePath)
	
	// Parse the declaration
	fmt.Println("Parsing declaration...")
	decl, err := parser.ParseFile(filePath)
	if err != nil {
		return fmt.Errorf("parse error: %w", err)
	}
	fmt.Printf("Found: %s -> %s\n", decl.RequestType, decl.ResponseType)

	// Gather context
	fmt.Println("Gathering context...")
	promptContext, err := prompt.GatherContext(filePath)
	if err != nil {
		return fmt.Errorf("context error: %w", err)
	}

	// Build prompt
	builder := prompt.NewBuilder(promptContext)
	fullPrompt := builder.Build(decl)

	// Create AI client
	config := &ai.Config{
		Model: viper.GetString("model"),
		Host:  viper.GetString("host"),
	}
	
	fmt.Printf("Creating AI client (model: %s)...\n", config.Model)
	aiClient, err := ai.NewClient(config)
	if err != nil {
		return fmt.Errorf("failed to create AI client: %w", err)
	}

	// Generate implementation
	fmt.Println("Generating implementation...")
	ctx := context.Background()
	response, err := aiClient.Generate(ctx, fullPrompt)
	if err != nil {
		return fmt.Errorf("generation error: %w", err)
	}

	// Write the file
	gen := generator.New(filePath)
	outputPath := gen.GetOutputPath()
	
	fmt.Printf("Writing to: %s\n", outputPath)
	err = gen.Generate(response)
	if err != nil {
		return fmt.Errorf("write error: %w", err)
	}

	fmt.Println("✓ Implementation generated successfully!")
	return nil
}