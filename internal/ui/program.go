package ui

import (
	"fmt"
	"log/slog"
	"os"

	tea "github.com/charmbracelet/bubbletea"
	"golang.org/x/term"
)

// ProgramOptions contains options for creating a Program
type ProgramOptions struct {
	Plain    bool       // Use plain text output instead of TUI
	LogLevel slog.Level // Current log level
}

// Program manages the TUI program and provides logger creation
type Program struct {
	model      *Model
	teaProgram *tea.Program
	isTerminal bool       // Whether stdout is a terminal
	plain      bool       // Whether to use plain text output
	logLevel   slog.Level // Current log level for filtering
}

// IsTerminal returns whether the program is running in a terminal
func (p *Program) IsTerminal() bool {
	return p.isTerminal
}

// IsTUIEnabled returns whether the TUI is enabled
func (p *Program) IsTUIEnabled() bool {
	return p.isTerminal && !p.plain
}

// NewProgram creates a new TUI program with default options
func NewProgram() *Program {
	return NewProgramWithOptions(ProgramOptions{})
}

// NewProgramWithOptions creates a new TUI program with specified options
func NewProgramWithOptions(opts ProgramOptions) *Program {
	model := newModel()

	// Check if stdout is a terminal
	isTerminal := term.IsTerminal(int(os.Stdout.Fd()))

	// Set log level in the model
	model.setLogLevel(opts.LogLevel)

	var teaProgram *tea.Program
	if opts.Plain || !isTerminal {
		// Plain mode or non-terminal mode - disable TUI rendering
		teaProgram = tea.NewProgram(model, tea.WithInput(nil), tea.WithoutRenderer())
	} else {
		// Normal terminal mode - standard TUI setup
		// We don't use alt screen to keep previous logs visible
		teaProgram = tea.NewProgram(model)
	}

	return &Program{
		model:      model,
		teaProgram: teaProgram,
		isTerminal: isTerminal,
		plain:      opts.Plain,
		logLevel:   opts.LogLevel,
	}
}

// Start starts the TUI program (blocks until Quit is called)
func (p *Program) Start() error {
	// Run the program (blocks until quit in terminal mode, returns immediately in non-terminal mode)
	_, err := p.teaProgram.Run()
	return err
}

// AddTarget registers a new target for UI tracking
func (p *Program) AddTarget(name string, index, total int) {
	// Add target to model
	p.model.addTarget(name, index, total)

	// Print initial progress message for plain output
	if p.shouldShowPlainOutput() {
		fmt.Fprintf(os.Stderr, "[%d/%d] Processing: %s\n", index, total, name)
	}
}

// SendLog sends a log message to the TUI
func (p *Program) SendLog(targetIndex int, level slog.Level, message string) {
	p.teaProgram.Send(logMsg{
		TargetIndex: targetIndex,
		Level:       level,
		Message:     message,
	})
}

// shouldShowPlainOutput returns true if plain text output should be shown
func (p *Program) shouldShowPlainOutput() bool {
	return p.plain || !p.isTerminal
}

// printProgress prints a progress message for plain output mode
func (p *Program) printProgress(targetIndex int, format string, args ...interface{}) {
	if !p.shouldShowPlainOutput() {
		return
	}

	p.model.mu.RLock()
	defer p.model.mu.RUnlock()

	if targetIndex > 0 && targetIndex <= len(p.model.targets) {
		target := p.model.targets[targetIndex-1]
		prefix := fmt.Sprintf("[%d/%d]", targetIndex, len(p.model.targets))
		if format == "" {
			fmt.Fprintf(os.Stderr, "%s %s\n", prefix, target.Name)
		} else {
			msg := fmt.Sprintf(format, args...)
			fmt.Fprintf(os.Stderr, "%s %s: %s\n", prefix, msg, target.Name)
		}
	}
}

// MarkAsRunning marks a target as running
func (p *Program) MarkAsRunning(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "running",
	})

	p.printProgress(targetIndex, "Processing")
}

// Complete marks a target as completed
func (p *Program) Complete(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "completed",
	})

	p.printProgress(targetIndex, "Completed")
}

// Fail marks a target as failed
func (p *Program) Fail(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "failed",
	})

	p.printProgress(targetIndex, "Failed")
}

// UpdatePhase updates the phase information for a target
func (p *Program) UpdatePhase(targetIndex int, phase string, detail string) {
	p.teaProgram.Send(phaseMsg{
		TargetIndex: targetIndex,
		Phase:       phase,
		Detail:      detail,
	})

	// Print phase update for plain output
	if p.shouldShowPlainOutput() {
		p.model.mu.RLock()
		if targetIndex > 0 && targetIndex <= len(p.model.targets) {
			target := p.model.targets[targetIndex-1]
			fmt.Fprintf(os.Stderr, "[%d/%d] %s - %s: %s\n", targetIndex, len(p.model.targets), target.Name, phase, detail)
		}
		p.model.mu.RUnlock()
	}
}

// Quit stops the TUI program
func (p *Program) Quit() {
	if p.isTerminal {
		p.teaProgram.Quit()
	}
}

// GetFailedTargets returns information about all failed targets
func (p *Program) GetFailedTargets() []*TargetView {
	p.model.mu.RLock()
	defer p.model.mu.RUnlock()

	var failed []*TargetView
	for _, target := range p.model.targets {
		if target.Status == "failed" {
			failed = append(failed, target)
		}
	}
	return failed
}

// GetAllTargets returns information about all targets
func (p *Program) GetAllTargets() []*TargetView {
	p.model.mu.RLock()
	defer p.model.mu.RUnlock()

	all := append([]*TargetView(nil), p.model.targets...)
	return all
}
