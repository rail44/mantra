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

// FormatRecord formats a log record into level string and message with attributes
func (h *BaseHandler) FormatRecord(r slog.Record) (levelStr string, formattedMsg string) {
	// Format level (only show for non-INFO levels)
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
	formattedMsg = r.Message
	r.Attrs(func(a slog.Attr) bool {
		// Skip time attribute
		if a.Key == slog.TimeKey {
			return true
		}
		// Format other attributes inline
		formattedMsg += fmt.Sprintf(" %s=%v", a.Key, a.Value.Any())
		return true
	})

	return levelStr, formattedMsg
}

// WriterHandler implements a log handler that writes to an io.Writer
type WriterHandler struct {
	BaseHandler
	output io.Writer
}

// NewWriterHandler creates a new handler that writes to the given output
func NewWriterHandler(output io.Writer, level slog.Level) *WriterHandler {
	return &WriterHandler{
		BaseHandler: BaseHandler{level: level},
		output:      output,
	}
}

func (h *WriterHandler) Handle(ctx context.Context, r slog.Record) error {
	h.mu.Lock()
	defer h.mu.Unlock()

	levelStr, formattedMsg := h.FormatRecord(r)

	// Write formatted message
	_, err := fmt.Fprintf(h.output, "%s%s\n", levelStr, formattedMsg)
	return err
}

// WithAttrs returns a new Handler whose attributes consist of both the receiver's attributes and the arguments
func (h *WriterHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	// For simplicity, we don't support WithAttrs
	return h
}

// WithGroup returns a new Handler with the given group name
func (h *WriterHandler) WithGroup(name string) slog.Handler {
	// For simplicity, we don't support WithGroup
	return h
}

// CallbackHandler is a slog.Handler that forwards log records to a callback function
type CallbackHandler struct {
	BaseHandler
	callback CallbackFunc
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

	// Simply forward the record to the callback
	h.callback(record)
	return nil
}

// WithAttrs returns a new Handler whose attributes consist of both the receiver's attributes and the arguments
func (h *CallbackHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	// For simplicity, we don't support WithAttrs
	return h
}

// WithGroup returns a new Handler with the given group name
func (h *CallbackHandler) WithGroup(name string) slog.Handler {
	// For simplicity, we don't support WithGroup
	return h
}

// PlainTargetHandler is a slog.Handler for plain mode output with target information
type PlainTargetHandler struct {
	BaseHandler
	targetNum    int
	totalTargets int
	targetName   string
	output       io.Writer
}

// NewPlainTargetHandler creates a new handler for plain mode with target information
func NewPlainTargetHandler(targetNum, totalTargets int, targetName string, output io.Writer, level slog.Level) *PlainTargetHandler {
	return &PlainTargetHandler{
		BaseHandler:  BaseHandler{level: level},
		targetNum:    targetNum,
		totalTargets: totalTargets,
		targetName:   targetName,
		output:       output,
	}
}

// Handle processes the Record and outputs formatted log
func (h *PlainTargetHandler) Handle(ctx context.Context, r slog.Record) error {
	h.mu.Lock()
	defer h.mu.Unlock()

	// Format level prefix and message
	levelStr, formattedMsg := h.FormatRecord(r)

	// Format target info
	targetInfo := fmt.Sprintf("[%d/%d %s]", h.targetNum, h.totalTargets, h.targetName)

	// Write to output
	fmt.Fprintf(h.output, "%s%s %s\n", levelStr, targetInfo, formattedMsg)
	return nil
}

// WithAttrs returns a new Handler with the given attributes
func (h *PlainTargetHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	// For simplicity, we don't support WithAttrs
	return h
}

// WithGroup returns a new Handler with the given group name
func (h *PlainTargetHandler) WithGroup(name string) slog.Handler {
	// For simplicity, we don't support WithGroup
	return h
}

// PlainLogger is a logger that outputs formatted logs for plain mode
type PlainLogger struct {
	targetNum    int
	totalTargets int
	targetName   string
	output       io.Writer
	level        slog.Level
}

// NewPlainLogger creates a logger for plain mode output
func NewPlainLogger(targetNum, totalTargets int, targetName string, output io.Writer, level slog.Level) Logger {
	return &PlainLogger{
		targetNum:    targetNum,
		totalTargets: totalTargets,
		targetName:   targetName,
		output:       output,
		level:        level,
	}
}

// HandleRecord processes a log record directly
func (l *PlainLogger) HandleRecord(record slog.Record) {
	// Check log level
	if record.Level < l.level {
		return
	}

	// Format level prefix
	var levelStr string
	switch {
	case record.Level >= slog.LevelError:
		levelStr = "[ERROR] "
	case record.Level >= slog.LevelWarn:
		levelStr = "[WARN] "
	case record.Level >= slog.LevelInfo:
		levelStr = "" // No prefix for INFO
	case record.Level >= slog.LevelDebug:
		levelStr = "[DEBUG] "
	default:
		levelStr = "[TRACE] "
	}

	// Format target info
	targetInfo := fmt.Sprintf("[%d/%d %s]", l.targetNum, l.totalTargets, l.targetName)

	// Format message with attributes from the record
	formattedMsg := record.Message
	record.Attrs(func(a slog.Attr) bool {
		if a.Key != slog.TimeKey { // Skip time attribute
			formattedMsg += fmt.Sprintf(" %s=%v", a.Key, a.Value.Any())
		}
		return true
	})

	// Write to output
	fmt.Fprintf(l.output, "%s%s %s\n", levelStr, targetInfo, formattedMsg)
}

func (l *PlainLogger) formatAndWrite(level slog.Level, msg string, args ...any) {
	// Check log level
	if level < l.level {
		return
	}

	// Format level prefix
	var levelStr string
	switch {
	case level >= slog.LevelError:
		levelStr = "[ERROR] "
	case level >= slog.LevelWarn:
		levelStr = "[WARN] "
	case level >= slog.LevelInfo:
		levelStr = "" // No prefix for INFO
	case level >= slog.LevelDebug:
		levelStr = "[DEBUG] "
	default:
		levelStr = "[TRACE] "
	}

	// Format target info
	targetInfo := fmt.Sprintf("[%d/%d %s]", l.targetNum, l.totalTargets, l.targetName)

	// Format message with attributes
	formattedMsg := msg
	if len(args) > 0 {
		for i := 0; i < len(args)-1; i += 2 {
			formattedMsg += fmt.Sprintf(" %v=%v", args[i], args[i+1])
		}
	}

	// Write to output
	fmt.Fprintf(l.output, "%s%s %s\n", levelStr, targetInfo, formattedMsg)
}

func (l *PlainLogger) Info(msg string, args ...any) {
	l.formatAndWrite(slog.LevelInfo, msg, args...)
}

func (l *PlainLogger) Debug(msg string, args ...any) {
	l.formatAndWrite(slog.LevelDebug, msg, args...)
}

func (l *PlainLogger) Error(msg string, args ...any) {
	l.formatAndWrite(slog.LevelError, msg, args...)
}

func (l *PlainLogger) Warn(msg string, args ...any) {
	l.formatAndWrite(slog.LevelWarn, msg, args...)
}

func (l *PlainLogger) Trace(msg string, args ...any) {
	l.formatAndWrite(slog.LevelDebug-4, msg, args...)
}

// slogLogger wraps slog.Logger to implement our Logger interface
type slogLogger struct {
	logger *slog.Logger
}

func (l *slogLogger) Info(msg string, args ...any) {
	l.logger.Info(msg, args...)
}

func (l *slogLogger) Debug(msg string, args ...any) {
	l.logger.Debug(msg, args...)
}

func (l *slogLogger) Error(msg string, args ...any) {
	l.logger.Error(msg, args...)
}

func (l *slogLogger) Warn(msg string, args ...any) {
	l.logger.Warn(msg, args...)
}

func (l *slogLogger) Trace(msg string, args ...any) {
	// slog doesn't have trace level, so we use Debug-4
	// We need to use Log method with custom level
	l.logger.Log(context.Background(), slog.LevelDebug-4, msg, args...)
}
