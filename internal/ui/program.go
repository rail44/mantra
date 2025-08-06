package ui

import (
	tea "github.com/charmbracelet/bubbletea"
)

// Program manages the TUI program and provides logger creation
type Program struct {
	model     *Model
	teaProgram *tea.Program
}

// NewProgram creates a new TUI program
func NewProgram() *Program {
	model := newModel()
	return &Program{
		model:     model,
		teaProgram: tea.NewProgram(model), // Remove WithAltScreen to keep output in terminal
	}
}

// Start starts the TUI program (blocks until Quit is called)
func (p *Program) Start() error {
	// Run the program (blocks until quit)
	_, err := p.teaProgram.Run()
	return err
}

// CreateTargetLogger creates a logger for a specific target
func (p *Program) CreateTargetLogger(name string, index, total int) TargetLogger {
	// Add target to model
	p.model.addTarget(name, index, total)
	
	// No longer auto-start TUI here - Start() must be called explicitly
	return newTargetLogger(p, name, index)
}

// sendLog sends a log message to the TUI
func (p *Program) sendLog(targetIndex int, level, message string) {
	p.teaProgram.Send(logMsg{
		TargetIndex: targetIndex,
		Level:       level,
		Message:     message,
	})
}

// Complete marks a target as completed
func (p *Program) Complete(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "completed",
	})
}

// Fail marks a target as failed
func (p *Program) Fail(targetIndex int) {
	p.teaProgram.Send(statusMsg{
		TargetIndex: targetIndex,
		Status:      "failed",
	})
}

// Quit stops the TUI program
func (p *Program) Quit() {
	p.teaProgram.Quit()
}