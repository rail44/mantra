package log

import "log/slog"

// CallbackFunc is a function that receives log messages
type CallbackFunc func(level slog.Level, msg string, args []any)

// CallbackLogger is a logger that forwards all logs to a callback function
type CallbackLogger struct {
	callback CallbackFunc
	minLevel slog.Level
}

// NewCallbackLogger creates a new logger that forwards logs to a callback function
func NewCallbackLogger(callback CallbackFunc, minLevel slog.Level) Logger {
	return &CallbackLogger{
		callback: callback,
		minLevel: minLevel,
	}
}

func (l *CallbackLogger) Info(msg string, args ...any) {
	if slog.LevelInfo >= l.minLevel && l.callback != nil {
		l.callback(slog.LevelInfo, msg, args)
	}
}

func (l *CallbackLogger) Debug(msg string, args ...any) {
	if slog.LevelDebug >= l.minLevel && l.callback != nil {
		l.callback(slog.LevelDebug, msg, args)
	}
}

func (l *CallbackLogger) Error(msg string, args ...any) {
	if slog.LevelError >= l.minLevel && l.callback != nil {
		l.callback(slog.LevelError, msg, args)
	}
}

func (l *CallbackLogger) Warn(msg string, args ...any) {
	if slog.LevelWarn >= l.minLevel && l.callback != nil {
		l.callback(slog.LevelWarn, msg, args)
	}
}

func (l *CallbackLogger) Trace(msg string, args ...any) {
	const traceLevel = slog.LevelDebug - 4
	if traceLevel >= l.minLevel && l.callback != nil {
		l.callback(traceLevel, msg, args)
	}
}
