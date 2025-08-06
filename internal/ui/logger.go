package ui

import (
	"fmt"
	"log/slog"
	"time"

	"github.com/rail44/mantra/internal/log"
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

func (l *targetLogger) log(level, msg string, args ...any) {
	// Check if this log level should be displayed
	currentLevel := log.GetCurrentLevel()
	var logLevel slog.Level
	
	switch level {
	case "ERROR":
		logLevel = slog.LevelError
	case "WARN":
		logLevel = slog.LevelWarn
	case "INFO":
		logLevel = slog.LevelInfo
	case "DEBUG":
		logLevel = slog.LevelDebug
	case "TRACE":
		logLevel = slog.LevelDebug - 4
	default:
		logLevel = slog.LevelInfo
	}
	
	// Skip if log level is too verbose for current setting
	if logLevel < currentLevel {
		return
	}
	
	// Format message with args if provided
	formattedMsg := msg
	if len(args) > 0 {
		// Simple key-value formatting
		formattedMsg = fmt.Sprintf("%s %v", msg, args)
	}

	// Send to the program
	l.program.sendLog(l.targetIndex, level, formattedMsg)
}

func (l *targetLogger) Info(msg string, args ...any) {
	l.log("INFO", msg, args...)
}

func (l *targetLogger) Debug(msg string, args ...any) {
	l.log("DEBUG", msg, args...)
}

func (l *targetLogger) Error(msg string, args ...any) {
	l.log("ERROR", msg, args...)
}

func (l *targetLogger) Warn(msg string, args ...any) {
	l.log("WARN", msg, args...)
}

func (l *targetLogger) Trace(msg string, args ...any) {
	l.log("TRACE", msg, args...)
}

// LogEntry represents a single log message
type LogEntry struct {
	Level     string
	Message   string
	Timestamp time.Time
}