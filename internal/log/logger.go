package log

import (
	"context"
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
	LevelTrace LogLevel = "trace"
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
	case LevelTrace:
		currentLevel = slog.LevelDebug - 4 // More verbose than debug
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
	case LevelError, LevelWarn, LevelInfo, LevelDebug, LevelTrace:
		return level, nil
	default:
		return "", fmt.Errorf("invalid log level: %s", s)
	}
}

func setupLogger(output io.Writer) {
	opts := &slog.HandlerOptions{
		Level: currentLevel,
		ReplaceAttr: func(groups []string, a slog.Attr) slog.Attr {
			// Remove time for cleaner CLI output
			if a.Key == slog.TimeKey {
				return slog.Attr{}
			}
			// Simplify level output
			if a.Key == slog.LevelKey {
				level := a.Value.Any().(slog.Level)
				switch {
				case level >= slog.LevelError:
					return slog.String(a.Key, "ERROR")
				case level >= slog.LevelWarn:
					return slog.String(a.Key, "WARN")
				case level >= slog.LevelInfo:
					return slog.String(a.Key, "INFO")
				case level >= slog.LevelDebug:
					return slog.String(a.Key, "DEBUG")
				default:
					return slog.String(a.Key, "TRACE")
				}
			}
			return a
		},
	}
	
	handler := slog.NewTextHandler(output, opts)
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

// Trace logs a trace message (most verbose)
func Trace(msg string, args ...any) {
	logger.Log(context.Background(), slog.LevelDebug-4, msg, args...)
}

// IsDebugEnabled returns true if debug logging is enabled
func IsDebugEnabled() bool {
	return currentLevel <= slog.LevelDebug
}