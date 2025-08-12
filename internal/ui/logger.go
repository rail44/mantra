package ui

import (
	"fmt"
	"log/slog"
	"time"
)

// TargetLogger provides logging for a specific generation target
type TargetLogger interface {
	Info(msg string, args ...any)
	Debug(msg string, args ...any)
	Error(msg string, args ...any)
	Warn(msg string, args ...any)
	Trace(msg string, args ...any)
}

// targetLogger implements TargetLogger
type targetLogger struct {
	targetIndex int
	targetName  string
	program     *Program
}

// newTargetLogger creates a new target logger
func newTargetLogger(program *Program, name string, index int) *targetLogger {
	return &targetLogger{
		targetIndex: index,
		targetName:  name,
		program:     program,
	}
}

func (l *targetLogger) log(level slog.Level, msg string, args ...any) {
	// Format message with args if provided
	formattedMsg := msg
	if len(args) > 0 {
		// Simple key-value formatting
		formattedMsg = fmt.Sprintf("%s %v", msg, args)
	}

	// Always send to the program for storage
	// The model will decide what to display in real-time vs store for later
	l.program.sendLog(l.targetIndex, level, formattedMsg)
}

func (l *targetLogger) Info(msg string, args ...any) {
	l.log(slog.LevelInfo, msg, args...)
}

func (l *targetLogger) Debug(msg string, args ...any) {
	l.log(slog.LevelDebug, msg, args...)
}

func (l *targetLogger) Error(msg string, args ...any) {
	l.log(slog.LevelError, msg, args...)
}

func (l *targetLogger) Warn(msg string, args ...any) {
	l.log(slog.LevelWarn, msg, args...)
}

func (l *targetLogger) Trace(msg string, args ...any) {
	l.log(slog.LevelDebug-4, msg, args...) // Same as in log package
}

// LogEntry represents a single log message
type LogEntry struct {
	Level     slog.Level
	Message   string
	Timestamp time.Time
}
