package log

import (
	"log/slog"
)

// CallbackFunc is a function that receives log records
type CallbackFunc func(record slog.Record)

// NewCallbackLogger creates a logger that forwards logs to a callback function
// This uses slog.Handler internally, so all logs are processed through the handler pipeline
func NewCallbackLogger(callback CallbackFunc, minLevel slog.Level) Logger {
	handler := NewCallbackHandler(callback, minLevel)
	slogger := slog.New(handler)
	return &slogLogger{logger: slogger}
}
