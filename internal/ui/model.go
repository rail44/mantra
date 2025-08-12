package ui

import (
	"context"
	"fmt"
	"log/slog"
	"strings"
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
	Logs        []slog.Record
	StartTime   time.Time
	EndTime     time.Time
}

// GetAllLogs returns a copy of all logs for the target
func (t *TargetView) GetAllLogs() []slog.Record {
	// Create a copy to avoid data races
	logs := make([]slog.Record, len(t.Logs))
	copy(logs, t.Logs)
	return logs
}

// Model is the Bubble Tea model for the TUI
type Model struct {
	targets    []*TargetView
	width      int
	height     int
	tuiEnabled bool
}

// newModel creates a new TUI model
func newModel(tuiEnabled bool) *Model {
	return &Model{
		targets:    make([]*TargetView, 0),
		tuiEnabled: tuiEnabled,
	}
}

func (m *Model) IsTUIEnabled() bool {
	return m.tuiEnabled
}

// addTarget adds a new target to track
func (m *Model) addTarget(name string, index, total int) {
	target := &TargetView{
		Name:      name,
		Index:     index,
		Total:     total,
		Status:    "pending",
		Phase:     "Initializing",
		Logs:      make([]slog.Record, 0),
		StartTime: time.Now(),
	}
	m.targets = append(m.targets, target)
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

	case addTargetMsg:
		// Add new target
		m.addTarget(msg.Name, msg.Index, msg.Total)
	}

	return m, nil
}

// View renders the UI
func (m *Model) View() string {
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
			// Active target - show with current status
			spinner := m.getSpinner(target.Status)

			// Format phase/step info to be right-aligned
			phaseInfo := ""
			if target.Phase != "" && target.Phase != "Initializing" {
				phaseInfo = fmt.Sprintf("[%s", target.Phase)
				if target.PhaseDetail != "" {
					phaseInfo += fmt.Sprintf(": %s", target.PhaseDetail)
				}
				phaseInfo += "]"
			}

			// Calculate padding for right alignment
			baseText := fmt.Sprintf("%s %s", spinner, target.Name)
			if phaseInfo != "" && m.width > 0 {
				// Calculate available space for padding
				totalLen := len(baseText) + len(phaseInfo)
				if totalLen < m.width-2 {
					padding := m.width - 2 - totalLen
					targetLine = fmt.Sprintf("%s%*s%s", baseText, padding, "", phaseInfo)
				} else {
					// If not enough space, just append normally
					targetLine = fmt.Sprintf("%s %s", baseText, phaseInfo)
				}
			} else {
				targetLine = baseText
			}

			// Always add log area (show latest log or placeholder)
			logFound := false
			if len(target.Logs) > 0 {
				// Show the latest log entry (already filtered by CallbackLogger)
				log := target.Logs[len(target.Logs)-1]
				// Truncate long messages for cleaner display
				msg := log.Message
				if len(msg) > 60 {
					msg = msg[:57] + "..."
				}
				targetLine += fmt.Sprintf("\n    • %s", msg)
				logFound = true
			}
			// If no log to show, add empty line to maintain consistent spacing
			if !logFound {
				targetLine += "\n"
			}

			activeTargets = append(activeTargets, targetLine)
		} else {
			// Completed/failed - show in compact form
			icon := m.getCompletionIcon(target.Status)
			duration := target.EndTime.Sub(target.StartTime).Round(time.Millisecond)
			targetLine = fmt.Sprintf("%s %s (%s)", icon, target.Name, duration)

			// Add final result message as a separate indented line (same as active targets)
			logFound := false
			if len(target.Logs) > 0 {
				// Show the latest log entry (already filtered by CallbackLogger)
				log := target.Logs[len(target.Logs)-1]
				// Truncate long messages for cleaner display
				msg := log.Message
				if len(msg) > 60 {
					msg = msg[:57] + "..."
				}
				targetLine += fmt.Sprintf("\n    • %s", msg)
				logFound = true
			}
			// For completed targets, show a result message if no log found
			if !logFound {
				if target.Status == "completed" {
					targetLine += "\n    • Completed successfully"
				} else if target.Status == "failed" {
					targetLine += "\n    • Failed"
				}
			}

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
	if !m.validateTargetIndex(msg.TargetIndex) {
		return
	}

	target := m.targets[msg.TargetIndex-1]
	target.Logs = append(target.Logs, msg.Record)

	// Check for phase/step information in the log record
	var phase, step string
	msg.Record.Attrs(func(a slog.Attr) bool {
		switch a.Key {
		case "phase":
			phase = a.Value.String()
		case "step":
			step = a.Value.String()
		}
		return true
	})

	// Update phase/step if present
	if phase != "" || step != "" {
		if phase != "" {
			target.Phase = phase
		}
		if step != "" {
			target.PhaseDetail = step
		}
	}

	if !m.tuiEnabled {
		m.PlainLog(msg.Record)
	}
}

func (m *Model) PlainLog(record slog.Record) {
	slog.Default().Handler().Handle(context.Background(), record)
}

func (m *Model) updateStatus(msg statusMsg) {
	if !m.validateTargetIndex(msg.TargetIndex) {
		return
	}

	target := m.targets[msg.TargetIndex-1]
	target.Status = msg.Status
	if msg.Status == "completed" || msg.Status == "failed" {
		target.EndTime = time.Now()
	}
}

// Message types
type tickMsg time.Time

type logMsg struct {
	TargetIndex int
	Record      slog.Record
}

type statusMsg struct {
	TargetIndex int
	Status      string
}

type addTargetMsg struct {
	Name  string
	Index int
	Total int
}

// GetFailedTargets returns all failed targets
func (m *Model) GetFailedTargets() []*TargetView {
	var failed []*TargetView
	for _, target := range m.targets {
		if target.Status == "failed" {
			failed = append(failed, target)
		}
	}
	return failed
}
