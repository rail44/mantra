package log

// Logger is the common interface for all loggers
type Logger interface {
	Info(msg string, args ...any)
	Debug(msg string, args ...any)
	Error(msg string, args ...any)
	Warn(msg string, args ...any)
	Trace(msg string, args ...any)
}

// Default returns the default logger that uses package-level functions
func Default() Logger {
	return &defaultLogger{}
}

// defaultLogger wraps the package-level logging functions
type defaultLogger struct{}

func (d *defaultLogger) Info(msg string, args ...any) {
	Info(msg, args...)
}

func (d *defaultLogger) Debug(msg string, args ...any) {
	Debug(msg, args...)
}

func (d *defaultLogger) Error(msg string, args ...any) {
	Error(msg, args...)
}

func (d *defaultLogger) Warn(msg string, args ...any) {
	Warn(msg, args...)
}

func (d *defaultLogger) Trace(msg string, args ...any) {
	Trace(msg, args...)
}