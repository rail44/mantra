package ui

import (
	"log/slog"
	"os"

	tea "github.com/charmbracelet/bubbletea"
	"golang.org/x/term"
)

// ProgramOptions contains options for creating a Program
type ProgramOptions struct {
	Plain bool // Use plain text output instead of TUI
}

// Program manages the TUI program and provides logger creation
type Program struct {
	teaProgram *tea.Program
}

// NewProgram creates a new TUI program with default options
func NewProgram() *Program {
	return NewProgramWithOptions(ProgramOptions{})
}

// NewProgramWithOptions creates a new TUI program with specified options
func NewProgramWithOptions(opts ProgramOptions) *Program {
	isTerminal := term.IsTerminal(int(os.Stdout.Fd()))

	// Determine if TUI should be enabled
	tuiEnabled := isTerminal && !opts.Plain
	model := newModel(tuiEnabled)

	var teaProgram *tea.Program
	if tuiEnabled {
		// Normal terminal mode - standard TUI setup
		// We don't use alt screen to keep previous logs visible
		teaProgram = tea.NewProgram(model)
	} else {
		// Plain mode or non-terminal mode - disable TUI rendering
		// Still use tea.Program for event handling and model updates
		teaProgram = tea.NewProgram(model, tea.WithInput(nil), tea.WithoutRenderer())
	}

	program := &Program{
		teaProgram: teaProgram,
	}

	return program
}

// Start starts the TUI program (blocks until Quit is called)
// Returns the final model state after the program ends
func (p *Program) Start() (*Model, error) {
	// Run the program (blocks until quit in terminal mode, returns immediately in non-terminal mode)
	finalModel, err := p.teaProgram.Run()
	if err != nil {
		return nil, err
	}

	// Return the final model state
	if m, ok := finalModel.(*Model); ok {
		return m, nil
	}

	return nil, nil
}

// AddTarget registers a new target for UI tracking
func (p *Program) AddTarget(name string, index, total int) {
	// Send message to add target
	p.teaProgram.Send(addTargetMsg{
		Name:  name,
		Index: index,
		Total: total,
	})
}

// SendLog sends a log record to the TUI or outputs via plain handler
func (p *Program) SendLog(record slog.Record) {
	var targetIndex int
	record.Attrs(func(a slog.Attr) bool {
		if a.Key == "targetIndex" {
			targetIndex = int(a.Value.Int64())
			return false
		}
		return true
	})
	p.teaProgram.Send(logMsg{
		TargetIndex: targetIndex,
		Record:      record,
	})
}

// MarkAsRunning marks a target as running
func (p *Program) MarkAsRunning(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "running",
	})
	// Plain mode output is handled by Handler
}

// Complete marks a target as completed
func (p *Program) Complete(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "completed",
	})
	// Plain mode output is handled by Handler
}

// Fail marks a target as failed
func (p *Program) Fail(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "failed",
	})
	// Plain mode output is handled by Handler
}

// Quit stops the TUI program
func (p *Program) Quit() {
	p.teaProgram.Quit()
}
