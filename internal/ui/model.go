package ui

import (
	"fmt"
	"log/slog"
	"strings"
	"sync"
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

// TargetView represents the view state for a single target
type TargetView struct {
	Name        string
	Index       int
	Total       int
	Status      string
	Phase       string // Current phase (e.g., "Context Gathering", "Implementation")
	PhaseDetail string // Phase-specific detail (e.g., "Analyzing codebase", "Generating code")
	Logs        []LogEntry
	StartTime   time.Time
	EndTime     time.Time
	mu          sync.RWMutex
}

// GetAllLogs returns a copy of all logs for the target
func (t *TargetView) GetAllLogs() []LogEntry {
	t.mu.RLock()
	defer t.mu.RUnlock()

	// Create a copy to avoid data races
	logs := make([]LogEntry, len(t.Logs))
	copy(logs, t.Logs)
	return logs
}

// Model is the Bubble Tea model for the TUI
type Model struct {
	targets      []*TargetView
	width        int
	height       int
	mu           sync.RWMutex
	logLevel     slog.Level // Current log level for filtering
}

// newModel creates a new TUI model
func newModel() *Model {
	return &Model{
		targets:  make([]*TargetView, 0),
		logLevel: slog.LevelInfo, // Default to INFO
	}
}

// setLogLevel sets the log level for filtering
func (m *Model) setLogLevel(level slog.Level) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.logLevel = level
}

// shouldShowLog determines if a log should be shown based on the current log level
func (m *Model) shouldShowLog(level slog.Level) bool {
	// Use slog's built-in level comparison
	// Show the log if its level is >= current level
	return level >= m.logLevel
}

// addTarget adds a new target to track
func (m *Model) addTarget(name string, index, total int) {
	m.mu.Lock()
	defer m.mu.Unlock()

	target := &TargetView{
		Name:      name,
		Index:     index,
		Total:     total,
		Status:    "pending",
		Phase:     "Initializing",
		Logs:      make([]LogEntry, 0),
		StartTime: time.Now(),
	}
	m.targets = append(m.targets, target)
}

// updatePhase updates the phase information for a target
func (m *Model) updatePhase(targetIndex int, phase string, detail string) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if m.validateTargetIndex(targetIndex) {
		target := m.targets[targetIndex-1]
		target.mu.Lock()
		target.Phase = phase
		target.PhaseDetail = detail
		target.mu.Unlock()
	}
}

// Init initializes the model
func (m *Model) Init() tea.Cmd {
	// Refresh display periodically
	return tea.Tick(100*time.Millisecond, func(t time.Time) tea.Msg {
		return tickMsg(t)
	})
}

// Update handles messages and updates the model
func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		// Only handle quit commands
		switch msg.String() {
		case "q", "ctrl+c":
			return m, tea.Quit
		}

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height

	case tickMsg:
		// Refresh display
		return m, tea.Tick(100*time.Millisecond, func(t time.Time) tea.Msg {
			return tickMsg(t)
		})

	case logMsg:
		// Add log to the appropriate target
		m.addLog(msg)

	case statusMsg:
		// Update target status
		m.updateStatus(msg)

	case phaseMsg:
		// Update target phase
		m.updatePhase(msg.TargetIndex, msg.Phase, msg.Detail)
	}

	return m, nil
}

// View renders the UI
func (m *Model) View() string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if len(m.targets) == 0 {
		return "Initializing..."
	}

	// Build output with string builder for better performance
	var sb strings.Builder

	stats := m.calculateStatistics()
	header := m.buildHeader(stats)
	sb.WriteString(header)
	sb.WriteString("\n")

	activeTargets, completedTargets := m.categorizeTargets()

	// Show active targets first
	if len(activeTargets) > 0 {
		sb.WriteString("\nActive:\n")
		m.appendTargetLines(&sb, activeTargets)
	}

	// Show completed targets
	if len(completedTargets) > 0 {
		sb.WriteString("\nCompleted:\n")
		m.appendTargetLines(&sb, completedTargets)
	}

	return sb.String()
}

// appendTargetLines appends formatted target lines to the string builder
func (m *Model) appendTargetLines(sb *strings.Builder, targets []string) {
	for _, line := range targets {
		// Handle multi-line entries (target + log)
		lines := strings.Split(line, "\n")
		for i, l := range lines {
			displayLine := l
			if i == 0 {
				// First line gets standard indentation
				displayLine = "  " + l
			} else {
				// Subsequent lines already have their own indentation
				displayLine = "  " + l
			}
			
			// Truncate lines if they exceed terminal width
			if m.width > 0 && len(displayLine) > m.width {
				displayLine = displayLine[:m.width-3] + "..."
			}
			sb.WriteString(displayLine)
			sb.WriteString("\n")
		}
	}
}

// targetStats holds aggregated statistics about targets
type targetStats struct {
	completed     int
	failed        int
	running       int
	pending       int
	total         int
	totalDuration time.Duration
}

// calculateStatistics computes aggregate statistics for all targets
func (m *Model) calculateStatistics() targetStats {
	stats := targetStats{total: len(m.targets)}

	var earliestStart time.Time
	var latestEnd time.Time

	for _, target := range m.targets {
		switch target.Status {
		case "completed":
			stats.completed++
			if earliestStart.IsZero() || target.StartTime.Before(earliestStart) {
				earliestStart = target.StartTime
			}
			if target.EndTime.After(latestEnd) {
				latestEnd = target.EndTime
			}
		case "failed":
			stats.failed++
			if earliestStart.IsZero() || target.StartTime.Before(earliestStart) {
				earliestStart = target.StartTime
			}
			if !target.EndTime.IsZero() && target.EndTime.After(latestEnd) {
				latestEnd = target.EndTime
			}
		case "running":
			stats.running++
			if earliestStart.IsZero() || target.StartTime.Before(earliestStart) {
				earliestStart = target.StartTime
			}
		case "pending":
			stats.pending++
		}
	}

	// Calculate total elapsed time
	if !earliestStart.IsZero() {
		if stats.running > 0 {
			stats.totalDuration = time.Since(earliestStart)
		} else if !latestEnd.IsZero() {
			stats.totalDuration = latestEnd.Sub(earliestStart)
		}
	}

	return stats
}

// buildHeader creates the header with progress bar
func (m *Model) buildHeader(stats targetStats) string {
	// Build progress bar
	progressWidth := 30
	filledWidth := (stats.completed * progressWidth) / stats.total
	if filledWidth > progressWidth {
		filledWidth = progressWidth
	}

	progressBar := "["
	for i := 0; i < progressWidth; i++ {
		if i < filledWidth {
			progressBar += "="
		} else if i == filledWidth && stats.running > 0 {
			progressBar += ">"
		} else {
			progressBar += " "
		}
	}
	progressBar += "]"

	// Build header
	percentage := (stats.completed * 100) / stats.total
	header := fmt.Sprintf("%s %3d%% | %d/%d", progressBar, percentage, stats.completed, stats.total)

	// Add timing info
	if stats.totalDuration > 0 {
		header += fmt.Sprintf(" | %s", stats.totalDuration.Round(time.Millisecond))
	}

	// Add status counts
	if stats.failed > 0 {
		header += fmt.Sprintf(" | FAILED: %d", stats.failed)
	}

	return header
}

// categorizeTargets separates targets into active and completed lists
func (m *Model) categorizeTargets() (activeTargets, completedTargets []string) {
	for _, target := range m.targets {
		var targetLine string

		if target.Status == "running" || target.Status == "pending" {
			// Active target - show with current status and phase
			spinner := m.getSpinner(target.Status)
			targetLine = fmt.Sprintf("%s %s", spinner, target.Name)

			target.mu.RLock()
			
			// Add phase information if available
			if target.Phase != "" && target.Phase != "Initializing" {
				targetLine += fmt.Sprintf(" [%s", target.Phase)
				if target.PhaseDetail != "" {
					targetLine += fmt.Sprintf(": %s", target.PhaseDetail)
				}
				targetLine += "]"
			}
			
			// Always add log area (show latest log or placeholder)
			logFound := false
			if len(target.Logs) > 0 {
				// Find the latest log entry that should be shown based on log level
				for i := len(target.Logs) - 1; i >= 0; i-- {
					log := target.Logs[i]
					// Skip logs that shouldn't be shown based on log level
					if !m.shouldShowLog(log.Level) {
						continue
					}
					// Truncate long messages for cleaner display
					msg := log.Message
					if len(msg) > 60 {
						msg = msg[:57] + "..."
					}
					targetLine += fmt.Sprintf("\n    → %s", msg)
					logFound = true
					break
				}
			}
			// If no log to show, add empty line to maintain consistent spacing
			if !logFound {
				targetLine += "\n    → (waiting...)"
			}
			
			target.mu.RUnlock()

			activeTargets = append(activeTargets, targetLine)
		} else {
			// Completed/failed - show in compact form
			icon := m.getCompletionIcon(target.Status)
			duration := target.EndTime.Sub(target.StartTime).Round(time.Millisecond)
			targetLine = fmt.Sprintf("%s %s (%s)", icon, target.Name, duration)
			
			// Add final result message as a separate indented line (same as active targets)
			target.mu.RLock()
			logFound := false
			if len(target.Logs) > 0 {
				// Find the latest log entry that should be shown based on log level
				for i := len(target.Logs) - 1; i >= 0; i-- {
					log := target.Logs[i]
					// Skip logs that shouldn't be shown based on log level
					if !m.shouldShowLog(log.Level) {
						continue
					}
					// Truncate long messages for cleaner display
					msg := log.Message
					if len(msg) > 60 {
						msg = msg[:57] + "..."
					}
					targetLine += fmt.Sprintf("\n    → %s", msg)
					logFound = true
					break
				}
			}
			// For completed targets, show a result message if no log found
			if !logFound {
				if target.Status == "completed" {
					targetLine += "\n    → Completed successfully"
				} else if target.Status == "failed" {
					targetLine += "\n    → Failed"
				}
			}
			target.mu.RUnlock()
			
			completedTargets = append(completedTargets, targetLine)
		}
	}

	return activeTargets, completedTargets
}

func (m *Model) getSpinner(status string) string {
	if status == "running" {
		// Simple text spinner animation using ASCII
		frames := []string{"|", "/", "-", "\\"}
		// Use time to determine frame
		frame := (time.Now().UnixMilli() / 200) % int64(len(frames))
		return frames[frame]
	} else if status == "pending" {
		return "."
	}
	return " "
}

func (m *Model) getCompletionIcon(status string) string {
	switch status {
	case "completed":
		return "[OK]"
	case "failed":
		return "[FAIL]"
	default:
		return "[?]"
	}
}

// validateTargetIndex checks if the target index is valid
func (m *Model) validateTargetIndex(index int) bool {
	return index >= 1 && index <= len(m.targets)
}

func (m *Model) addLog(msg logMsg) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if !m.validateTargetIndex(msg.TargetIndex) {
		return
	}

	target := m.targets[msg.TargetIndex-1] // Convert to 0-based index
	target.mu.Lock()
	defer target.mu.Unlock()

	// Always store the log for later display
	target.Logs = append(target.Logs, LogEntry{
		Level:     msg.Level,
		Message:   msg.Message,
		Timestamp: time.Now(),
	})

	// Auto-update status on first log
	if target.Status == "pending" && msg.Level == slog.LevelInfo {
		target.Status = "running"
	}
}

func (m *Model) updateStatus(msg statusMsg) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if !m.validateTargetIndex(msg.TargetIndex) {
		return
	}

	target := m.targets[msg.TargetIndex-1]
	target.mu.Lock()
	defer target.mu.Unlock()

	target.Status = msg.Status
	if msg.Status == "completed" || msg.Status == "failed" {
		target.EndTime = time.Now()
	}
}

// Message types
type tickMsg time.Time

type logMsg struct {
	TargetIndex int
	Level       slog.Level
	Message     string
}

type statusMsg struct {
	TargetIndex int
	Status      string
}

type phaseMsg struct {
	TargetIndex int
	Phase       string
	Detail      string
}
