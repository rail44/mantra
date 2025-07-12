package cmd

import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
	
	"github.com/rail44/glyph/internal/ai"
	"github.com/rail44/glyph/internal/interactive"
)

var watchCmd = &cobra.Command{
	Use:   "watch <file>",
	Short: "Watch a declaration file and generate implementation in real-time",
	Long: `Watch monitors changes to a Go declaration file and automatically
regenerates the implementation using AI whenever the file is saved.`,
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

		// Run the interactive watcher
		if err := runInteractiveMode(absPath); err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
	},
}

func init() {
	rootCmd.AddCommand(watchCmd)
}

func runInteractiveMode(filePath string) error {
	// Create AI client with config
	config := &ai.Config{
		Model:   viper.GetString("model"),
		Host:    viper.GetString("host"),
	}
	
	aiClient, err := ai.NewClient(config)
	if err != nil {
		return fmt.Errorf("failed to create AI client: %w", err)
	}

	// Create the Bubble Tea model
	m := interactive.NewModel(filePath, aiClient)
	
	// Create the Bubble Tea program
	p := tea.NewProgram(m, tea.WithAltScreen())
	
	// Create file watcher
	watcher, err := interactive.NewFileWatcher(filePath, func() {
		p.Send(interactive.FileChanged())
	})
	if err != nil {
		return fmt.Errorf("failed to create file watcher: %w", err)
	}
	defer watcher.Close()

	// Start watching in background
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	
	go watcher.Start(ctx)

	// Trigger initial generation
	p.Send(interactive.FileChanged())

	// Run the UI
	if _, err := p.Run(); err != nil {
		return fmt.Errorf("failed to run UI: %w", err)
	}

	return nil
}