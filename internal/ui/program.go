package ui

import (
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
	// Plain mode output is handled by PlainLogger
}

// SendLog sends a log message to the TUI
func (p *Program) SendLog(targetIndex int, level slog.Level, message string) {
	// In TUI mode only - plain mode uses PlainLogger directly
	p.teaProgram.Send(logMsg{
		TargetIndex: targetIndex,
		Level:       level,
		Message:     message,
	})
}

// MarkAsRunning marks a target as running
func (p *Program) MarkAsRunning(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "running",
	})
	// Plain mode output is handled by PlainLogger
}

// Complete marks a target as completed
func (p *Program) Complete(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "completed",
	})
	// Plain mode output is handled by PlainLogger
}

// Fail marks a target as failed
func (p *Program) Fail(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "failed",
	})
	// Plain mode output is handled by PlainLogger
}

// UpdatePhase updates the phase information for a target
func (p *Program) UpdatePhase(targetIndex int, phase string, detail string) {
	p.teaProgram.Send(phaseMsg{
		TargetIndex: targetIndex,
		Phase:       phase,
		Detail:      detail,
	})
	// Plain mode output is handled by PlainLogger (phase updates are shown as regular logs)
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
