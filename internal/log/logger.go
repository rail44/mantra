package log

import (
	"fmt"
	"io"
	"log/slog"
	"os"
	"strings"
)

// LogLevel represents the logging level
type LogLevel string

const (
	LevelError LogLevel = "error"
	LevelWarn  LogLevel = "warn"
	LevelInfo  LogLevel = "info"
	LevelDebug LogLevel = "debug"
)

var (
	// Current logger instance
	logger *slog.Logger

	// Current log level
	currentLevel slog.Level
)

func init() {
	// Initialize with default settings
	SetLevel(LevelInfo)
}

// SetLevel configures the logging level
func SetLevel(level LogLevel) error {
	switch level {
	case LevelError:
		currentLevel = slog.LevelError
	case LevelWarn:
		currentLevel = slog.LevelWarn
	case LevelInfo:
		currentLevel = slog.LevelInfo
	case LevelDebug:
		currentLevel = slog.LevelDebug
	default:
		return fmt.Errorf("invalid log level: %s", level)
	}

	setupLogger(os.Stderr)
	return nil
}

// ParseLevel converts a string to LogLevel
func ParseLevel(s string) (LogLevel, error) {
	level := LogLevel(strings.ToLower(s))
	switch level {
	case LevelError, LevelWarn, LevelInfo, LevelDebug:
		return level, nil
	default:
		return "", fmt.Errorf("invalid log level: %s", s)
	}
}

func setupLogger(output io.Writer) {
	// Create a handler for cleaner output (without target info)
	handler := NewHandler(output, currentLevel)
	logger = slog.New(handler)
}

// Error logs an error message
func Error(msg string, args ...any) {
	logger.Error(msg, args...)
}

// Warn logs a warning message
func Warn(msg string, args ...any) {
	logger.Warn(msg, args...)
}

// Info logs an info message
func Info(msg string, args ...any) {
	logger.Info(msg, args...)
}

// Debug logs a debug message
func Debug(msg string, args ...any) {
	logger.Debug(msg, args...)
}

// IsDebugEnabled returns true if debug logging is enabled
func IsDebugEnabled() bool {
	return currentLevel <= slog.LevelDebug
}

// GetCurrentLevel returns the current log level
func GetCurrentLevel() slog.Level {
	return currentLevel
}
