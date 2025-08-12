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

var (
	plain    bool
	logLevel string
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
			slog.Error("failed to load configuration", slog.String("error", err.Error()))
			os.Exit(1)
		}

		// Set up logging
		setupLogging(cfg)

		// Ensure absolute path
		absPkgDir, err := filepath.Abs(pkgDir)
		if err != nil {
			slog.Error("failed to get absolute path", slog.String("error", err.Error()))
			os.Exit(1)
		}

		// Set plain output flag in config
		cfg.Plain = plain

		// Run generation
		generateApp := app.NewGenerateApp()
		if err := generateApp.Run(context.Background(), absPkgDir, cfg); err != nil {
			slog.Error("generation failed", slog.String("error", err.Error()))
			os.Exit(1)
		}
	},
}

func init() {
	generateCmd.Flags().BoolVar(&plain, "plain", false, "Use plain text output instead of interactive TUI")
	generateCmd.Flags().StringVar(&logLevel, "log-level", "", "Override log level (error, warn, info, debug, trace)")
	rootCmd.AddCommand(generateCmd)
}

func setupLogging(cfg *config.Config) {
	// Command line flag takes precedence over config file
	level := logLevel
	if level == "" {
		level = cfg.LogLevel
	}
	if level == "" {
		level = "info"
	}

	parsedLevel, err := log.ParseLevel(level)
	if err != nil {
		slog.Error("invalid log level", slog.String("level", level), slog.String("error", err.Error()))
		os.Exit(1)
	}
	log.Level.Set(parsedLevel)
}
