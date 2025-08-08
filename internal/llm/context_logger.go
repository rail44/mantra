package llm

import (
	"context"

	"github.com/rail44/mantra/internal/log"
)

// contextKey is a type for context keys to avoid collisions
type contextKey string

const (
	// loggerContextKey is the context key for storing the logger
	loggerContextKey contextKey = "llm-logger"
)

// ContextLogger is a thread-safe logger that retrieves the actual logger from context
type ContextLogger struct {
	fallbackLogger log.Logger
}

// NewContextLogger creates a new context-aware logger
func NewContextLogger(fallbackLogger log.Logger) *ContextLogger {
	return &ContextLogger{
		fallbackLogger: fallbackLogger,
	}
}

// getLogger retrieves the logger from context or returns the fallback
func (c *ContextLogger) getLogger(ctx context.Context) log.Logger {
	if logger, ok := ctx.Value(loggerContextKey).(log.Logger); ok {
		return logger
	}
	return c.fallbackLogger
}

// Info logs an info message using the logger from context
func (c *ContextLogger) Info(msg string, args ...any) {
	// Context is not available in the Logger interface, so we use the fallback
	// This is a limitation that we'll address by updating the interface
	c.fallbackLogger.Info(msg, args...)
}

// Debug logs a debug message using the logger from context
func (c *ContextLogger) Debug(msg string, args ...any) {
	c.fallbackLogger.Debug(msg, args...)
}

// Error logs an error message using the logger from context
func (c *ContextLogger) Error(msg string, args ...any) {
	c.fallbackLogger.Error(msg, args...)
}

// Warn logs a warning message using the logger from context
func (c *ContextLogger) Warn(msg string, args ...any) {
	c.fallbackLogger.Warn(msg, args...)
}

// Trace logs a trace message using the logger from context
func (c *ContextLogger) Trace(msg string, args ...any) {
	c.fallbackLogger.Trace(msg, args...)
}

// WithLogger returns a new context with the logger attached
func WithLogger(ctx context.Context, logger log.Logger) context.Context {
	return context.WithValue(ctx, loggerContextKey, logger)
}

// LoggerFromContext retrieves the logger from context
func LoggerFromContext(ctx context.Context, fallback log.Logger) log.Logger {
	if logger, ok := ctx.Value(loggerContextKey).(log.Logger); ok {
		return logger
	}
	return fallback
}
