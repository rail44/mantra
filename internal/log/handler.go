package log

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"sync"
)

// BaseHandler provides common formatting logic for all handlers
type BaseHandler struct {
	level slog.Level
	mu    sync.Mutex
}

// Enabled reports whether the handler handles records at the given level
func (h *BaseHandler) Enabled(_ context.Context, level slog.Level) bool {
	return level >= h.level
}

// CallbackHandler is a slog.Handler that forwards log records to a callback function
type CallbackHandler struct {
	BaseHandler
	callback CallbackFunc
	attrs    []slog.Attr
}

// NewCallbackHandler creates a new slog handler that forwards logs to a callback
func NewCallbackHandler(callback CallbackFunc, level slog.Level) *CallbackHandler {
	return &CallbackHandler{
		BaseHandler: BaseHandler{level: level},
		callback:    callback,
	}
}

// Handle handles the Record by forwarding to the callback
func (h *CallbackHandler) Handle(ctx context.Context, record slog.Record) error {
	h.mu.Lock()
	defer h.mu.Unlock()

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
		BaseHandler: h.BaseHandler,
		callback:    h.callback,
		attrs:       append(h.attrs, attrs...),
	}
}

// WithGroup returns a new Handler with the given group name
func (h *CallbackHandler) WithGroup(name string) slog.Handler {
	// For simplicity, we don't support WithGroup
	return h
}

// Handler is a slog.Handler for formatted output with optional target information
type Handler struct {
	level        slog.Level
	mu           sync.Mutex
	targetNum    int
	totalTargets int
	targetName   string
	output       io.Writer
}

// NewHandler creates a new handler for formatted output
func NewHandler(output io.Writer, level slog.Level) *Handler {
	return &Handler{
		level:  level,
		output: output,
	}
}

// NewHandlerWithTarget creates a new handler with target information
func NewHandlerWithTarget(targetNum, totalTargets int, targetName string, output io.Writer, level slog.Level) *Handler {
	return &Handler{
		level:        level,
		targetNum:    targetNum,
		totalTargets: totalTargets,
		targetName:   targetName,
		output:       output,
	}
}

// Enabled returns whether the handler handles records at the given level
func (h *Handler) Enabled(ctx context.Context, level slog.Level) bool {
	return level >= h.level
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

	// Format target info if available
	var targetInfo string
	if h.targetName != "" {
		targetInfo = fmt.Sprintf("[%d/%d %s] ", h.targetNum, h.totalTargets, h.targetName)
	}

	// Build the message with attributes, excluding target-related ones
	formattedMsg := r.Message
	r.Attrs(func(a slog.Attr) bool {
		// Skip target-related attributes and time
		if a.Key == "targetIndex" || a.Key == "totalTargets" || a.Key == "targetName" || a.Key == slog.TimeKey {
			return true
		}
		// Format other attributes inline
		formattedMsg += fmt.Sprintf(" %s=%v", a.Key, a.Value.Any())
		return true
	})

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
