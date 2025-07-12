package interactive

import (
	"context"
	"fmt"
	"path/filepath"
	"time"

	"github.com/fsnotify/fsnotify"
)

type FileWatcher struct {
	watcher  *fsnotify.Watcher
	filePath string
	onChange func()
}

func NewFileWatcher(filePath string, onChange func()) (*FileWatcher, error) {
	watcher, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, fmt.Errorf("failed to create watcher: %w", err)
	}

	// Watch the specific file
	err = watcher.Add(filePath)
	if err != nil {
		watcher.Close()
		return nil, fmt.Errorf("failed to watch file: %w", err)
	}

	// Also watch the directory for file recreation events
	dir := filepath.Dir(filePath)
	err = watcher.Add(dir)
	if err != nil {
		// Non-fatal: some editors recreate files
		fmt.Printf("Warning: couldn't watch directory %s: %v\n", dir, err)
	}

	return &FileWatcher{
		watcher:  watcher,
		filePath: filePath,
		onChange: onChange,
	}, nil
}

func (fw *FileWatcher) Start(ctx context.Context) {
	// Debounce timer to avoid multiple rapid events
	var debounceTimer *time.Timer
	const debounceDelay = 100 * time.Millisecond

	for {
		select {
		case <-ctx.Done():
			return
		case event, ok := <-fw.watcher.Events:
			if !ok {
				return
			}

			// Check if the event is for our file
			if filepath.Clean(event.Name) == filepath.Clean(fw.filePath) {
				if event.Op&(fsnotify.Write|fsnotify.Create) != 0 {
					// Cancel existing timer
					if debounceTimer != nil {
						debounceTimer.Stop()
					}
					
					// Set new timer
					debounceTimer = time.AfterFunc(debounceDelay, fw.onChange)
				}
			}
		case err, ok := <-fw.watcher.Errors:
			if !ok {
				return
			}
			fmt.Printf("Watcher error: %v\n", err)
		}
	}
}

func (fw *FileWatcher) Close() error {
	return fw.watcher.Close()
}