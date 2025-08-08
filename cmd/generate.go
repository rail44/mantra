package cmd

import (
	"context"
	"os"
	"path/filepath"

	"log/slog"

	"github.com/spf13/cobra"

	"github.com/rail44/mantra/internal/app"
	"github.com/rail44/mantra/internal/config"
	"github.com/rail44/mantra/internal/log"
)

var verbose bool

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
		setupLogging(cfg)

		// Ensure absolute path
		absPkgDir, err := filepath.Abs(pkgDir)
		if err != nil {
			log.Error("failed to get absolute path", slog.String("error", err.Error()))
			os.Exit(1)
		}

		// Set verbose flag in config
		cfg.Verbose = verbose

		// Run generation
		generateApp := app.NewGenerateApp()
		if err := generateApp.Run(context.Background(), absPkgDir, cfg); err != nil {
			log.Error("generation failed", slog.String("error", err.Error()))
			os.Exit(1)
		}
	},
}

func init() {
	generateCmd.Flags().BoolVarP(&verbose, "verbose", "v", false, "Show detailed logs for all targets")
	rootCmd.AddCommand(generateCmd)
}

func setupLogging(cfg *config.Config) {
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
}