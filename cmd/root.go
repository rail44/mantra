package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

var (
	modelName  string
	ollamaHost string
)

var rootCmd = &cobra.Command{
	Use:   "glyph",
	Short: "AI-powered Go code generator",
	Long: `Glyph is a local-first interactive development tool that generates
AI-powered Go code implementations from natural language instructions.`,
	CompletionOptions: cobra.CompletionOptions{
		DisableDefaultCmd: true,
	},
}

func Execute() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func init() {
	rootCmd.PersistentFlags().StringVar(&modelName, "model", "devstral", "AI model to use for generation")
	rootCmd.PersistentFlags().StringVar(&ollamaHost, "host", "http://localhost:11434", "Ollama host URL")
}

// GetModel returns the configured model name
func GetModel() string {
	return modelName
}

// GetHost returns the configured Ollama host
func GetHost() string {
	return ollamaHost
}
