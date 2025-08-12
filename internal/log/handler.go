package log

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"sync"
)

// CallbackFunc is a function that receives log records
type CallbackFunc func(record slog.Record)

// CallbackHandler is a slog.Handler that forwards log records to a callback function
type CallbackHandler struct {
	level    slog.Leveler // Use Leveler interface for dynamic level
	callback CallbackFunc
	attrs    []slog.Attr
}

// NewCallbackHandler creates a new slog handler that forwards logs to a callback
func NewCallbackHandler(callback CallbackFunc) *CallbackHandler {
	return &CallbackHandler{
		level:    Level,
		callback: callback,
	}
}

// Enabled reports whether the handler handles records at the given level
func (h *CallbackHandler) Enabled(_ context.Context, level slog.Level) bool {
	return level >= h.level.Level()
}

// Handle handles the Record by forwarding to the callback
func (h *CallbackHandler) Handle(ctx context.Context, record slog.Record) error {
	if h.callback == nil {
		return nil
	}

	// Add stored attributes to the record
	if len(h.attrs) > 0 {
		record.AddAttrs(h.attrs...)
	}

	// Forward the record to the callback
	h.callback(record)
	return nil
}

// WithAttrs returns a new Handler whose attributes consist of both the receiver's attributes and the arguments
func (h *CallbackHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	return &CallbackHandler{
		level:    h.level,
		callback: h.callback,
		attrs:    append(h.attrs, attrs...),
	}
}

// WithGroup returns a new Handler with the given group name
func (h *CallbackHandler) WithGroup(name string) slog.Handler {
	// For simplicity, we don't support WithGroup
	return h
}

// Handler is a slog.Handler for formatted output with optional target information
type Handler struct {
	level  slog.Leveler // Use Leveler interface for dynamic level
	mu     sync.Mutex
	output io.Writer
}

// NewHandler creates a new handler for formatted output
func NewHandler(output io.Writer) *Handler {
	return &Handler{
		level:  Level,
		output: output,
	}
}

// Enabled returns whether the handler handles records at the given level
func (h *Handler) Enabled(ctx context.Context, level slog.Level) bool {
	return level >= h.level.Level()
}

// Handle processes the Record and outputs formatted log
func (h *Handler) Handle(ctx context.Context, r slog.Record) error {
	h.mu.Lock()
	defer h.mu.Unlock()

	// Format level prefix
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

	formattedMsg := r.Message
	var targetIndex, totalTargets int
	var targetName string
	r.Attrs(func(a slog.Attr) bool {
		switch a.Key {
		case "targetIndex":
			targetIndex = int(a.Value.Int64())
		case "totalTargets":
			totalTargets = int(a.Value.Int64())
		case "targetName":
			targetName = a.Value.String()
		default:
			formattedMsg += fmt.Sprintf(" %s=%v", a.Key, a.Value.Any())
		}

		// Format other attributes inline
		return true
	})

	// Format target info if available
	var targetInfo string
	if targetName != "" {
		targetInfo = fmt.Sprintf("[%d/%d %s] ", targetIndex, totalTargets, targetName)
	}

	// Write to output
	fmt.Fprintf(h.output, "%s%s%s\n", levelStr, targetInfo, formattedMsg)
	return nil
}

// WithAttrs returns a new Handler with the given attributes
func (h *Handler) WithAttrs(attrs []slog.Attr) slog.Handler {
	// For simplicity, we don't support WithAttrs
	return h
}

// WithGroup returns a new Handler with the given group name
func (h *Handler) WithGroup(name string) slog.Handler {
	// For simplicity, we don't support WithGroup
	return h
}
