package log

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"os"
	"strings"
	"sync"
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

// customHandler implements a custom log handler for cleaner output
type customHandler struct {
	output io.Writer
	level  slog.Level
	mu     sync.Mutex
}

func (h *customHandler) Enabled(_ context.Context, level slog.Level) bool {
	return level >= h.level
}

func (h *customHandler) Handle(_ context.Context, r slog.Record) error {
	h.mu.Lock()
	defer h.mu.Unlock()

	// Format level (only show for non-INFO levels)
	var levelStr string
	switch {
	case r.Level >= slog.LevelError:
		levelStr = "[ERROR] "
	case r.Level >= slog.LevelWarn:
		levelStr = "[WARN] "
	case r.Level >= slog.LevelInfo:
		levelStr = "" // No prefix for INFO
	case r.Level >= slog.LevelDebug:
		levelStr = "[DEBUG] "
	default:
		levelStr = "[TRACE] "
	}

	// Build the message with attributes
	msg := r.Message
	r.Attrs(func(a slog.Attr) bool {
		// Skip time attribute
		if a.Key == slog.TimeKey {
			return true
		}
		// Format other attributes inline
		msg += fmt.Sprintf(" %s=%v", a.Key, a.Value.Any())
		return true
	})

	// Write formatted message
	_, err := fmt.Fprintf(h.output, "%s%s\n", levelStr, msg)
	return err
}

func (h *customHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	// For simplicity, we don't support WithAttrs in our custom handler
	return h
}

func (h *customHandler) WithGroup(name string) slog.Handler {
	// For simplicity, we don't support WithGroup in our custom handler
	return h
}

func setupLogger(output io.Writer) {
	// Create a custom handler for cleaner output
	handler := &customHandler{
		output: output,
		level:  currentLevel,
	}
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

// IsTraceEnabled returns true if trace logging is enabled
func IsTraceEnabled() bool {
	return currentLevel <= slog.LevelDebug-4
}

// GetCurrentLevel returns the current log level
func GetCurrentLevel() slog.Level {
	return currentLevel
}
