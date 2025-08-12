package log

import (
	"log/slog"
	"os"
)

// Default returns the default logger that uses package-level functions
func Default() *slog.Logger {
	handler := NewHandler(os.Stderr, GetCurrentLevel())
	return slog.New(handler)
}
