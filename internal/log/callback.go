package log

import (
	"log/slog"
)

// CallbackFunc is a function that receives log records
type CallbackFunc func(record slog.Record)

// NewCallbackLogger creates a logger that forwards logs to a callback function
// This uses slog.Handler internally, so all logs are processed through the handler pipeline
func NewCallbackLogger(callback CallbackFunc, minLevel slog.Level) *slog.Logger {
	handler := NewCallbackHandler(callback, minLevel)
	return slog.New(handler)
}

// NewCallbackLoggerWithAttrs creates a logger with pre-set attributes
func NewCallbackLoggerWithAttrs(callback CallbackFunc, minLevel slog.Level, attrs ...slog.Attr) *slog.Logger {
	handler := NewCallbackHandler(callback, minLevel).WithAttrs(attrs)
	return slog.New(handler)
}
