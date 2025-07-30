package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/rail44/mantra/internal/log"
)

var (
	modelName  string
	ollamaHost string
	logLevel   string
	provider   string
	apiKey     string
)

var rootCmd = &cobra.Command{
	Use:   "mantra",
	Short: "AI-powered Go code generator",
	Long: `Mantra is a local-first interactive development tool that generates
AI-powered Go code implementations from natural language instructions.`,
	CompletionOptions: cobra.CompletionOptions{
		DisableDefaultCmd: true,
	},
	PersistentPreRunE: func(cmd *cobra.Command, args []string) error {
		// Set up logging level
		level, err := log.ParseLevel(logLevel)
		if err != nil {
			return fmt.Errorf("invalid log level: %s", logLevel)
		}
		return log.SetLevel(level)
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
	rootCmd.PersistentFlags().StringVar(&logLevel, "log-level", "info", "Log level (error|warn|info|debug|trace)")
	rootCmd.PersistentFlags().StringVar(&provider, "provider", "ollama", "AI provider to use (ollama|openai)")
	rootCmd.PersistentFlags().StringVar(&apiKey, "api-key", "", "API key for OpenAI-compatible providers (can also use MANTRA_OPENAI_API_KEY env var)")
}

// GetModel returns the configured model name
func GetModel() string {
	return modelName
}

// GetHost returns the configured Ollama host
func GetHost() string {
	return ollamaHost
}

// GetProvider returns the configured provider
func GetProvider() string {
	return provider
}

// GetAPIKey returns the configured API key
func GetAPIKey() string {
	return apiKey
}
