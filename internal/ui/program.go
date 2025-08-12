package ui

import (
	"context"
	"log/slog"
	"os"

	tea "github.com/charmbracelet/bubbletea"
	"golang.org/x/term"

	"github.com/rail44/mantra/internal/log"
)

// ProgramOptions contains options for creating a Program
type ProgramOptions struct {
	Plain bool // Use plain text output instead of TUI
}

// Program manages the TUI program and provides logger creation
type Program struct {
	model        *Model
	teaProgram   *tea.Program
	tuiEnabled   bool                  // Whether TUI rendering is enabled
	plainLoggers map[int]*slog.Logger // Plain loggers for each target (plain mode only)
}

// IsTUIEnabled returns whether the TUI is enabled
func (p *Program) IsTUIEnabled() bool {
	return p.tuiEnabled
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

	// Determine if TUI should be enabled
	tuiEnabled := isTerminal && !opts.Plain

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
		model:      model,
		teaProgram: teaProgram,
		tuiEnabled: tuiEnabled,
	}

	// Initialize plain loggers map for plain mode
	if !tuiEnabled {
		program.plainLoggers = make(map[int]*slog.Logger)
	}

	return program
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

	// Create slog.Logger with PlainTargetHandler for this target in plain mode
	if !p.tuiEnabled {
		handler := log.NewPlainTargetHandler(index, total, name, os.Stderr, log.GetCurrentLevel())
		p.plainLoggers[index] = slog.New(handler)
	}
}

// SendLog sends a log record to the TUI or outputs via PlainLogger
func (p *Program) SendLog(targetIndex int, record slog.Record) {
	if p.tuiEnabled {
		// TUI mode: send to TUI
		p.teaProgram.Send(logMsg{
			TargetIndex: targetIndex,
			Record:      record,
		})
	} else {
		// Plain mode: output via slog.Logger
		if logger, ok := p.plainLoggers[targetIndex]; ok {
			// Use LogAttrs to preserve all record attributes
			var attrs []slog.Attr
			record.Attrs(func(a slog.Attr) bool {
				attrs = append(attrs, a)
				return true
			})
			logger.LogAttrs(context.Background(), record.Level, record.Message, attrs...)
		}
	}
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
	if p.tuiEnabled {
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
