package log

import (
	"fmt"
	"log/slog"
	"strings"
)

// Level is the global log level for all handlers.
// It can be changed dynamically using Level.Set(level).
var Level = new(slog.LevelVar) // Info by default

// ParseLevel converts a string to slog.Level
func ParseLevel(s string) (slog.Level, error) {
	switch strings.ToLower(s) {
	case "error":
		return slog.LevelError, nil
	case "warn":
		return slog.LevelWarn, nil
	case "info":
		return slog.LevelInfo, nil
	case "debug":
		return slog.LevelDebug, nil
	default:
		return slog.LevelInfo, fmt.Errorf("invalid log level: %s", s)
	}
}