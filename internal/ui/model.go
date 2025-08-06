package ui

import (
	"fmt"
	"strings"
	"sync"
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

// TargetView represents the view state for a single target
type TargetView struct {
	Name      string
	Index     int
	Total     int
	Status    string
	Logs      []LogEntry
	StartTime time.Time
	EndTime   time.Time
	mu        sync.RWMutex
}

// Model is the Bubble Tea model for the TUI
type Model struct {
	targets []*TargetView
	width   int
	height  int
	mu      sync.RWMutex
}

// newModel creates a new TUI model
func newModel() *Model {
	return &Model{
		targets: make([]*TargetView, 0),
	}
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
		Logs:      make([]LogEntry, 0),
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

	var lines []string
	
	// Count targets by status
	completed := 0
	failed := 0
	running := 0
	pending := 0
	
	// Calculate elapsed time from the first started target
	var earliestStart time.Time
	var latestEnd time.Time
	
	for _, target := range m.targets {
		switch target.Status {
		case "completed":
			completed++
			if earliestStart.IsZero() || target.StartTime.Before(earliestStart) {
				earliestStart = target.StartTime
			}
			if target.EndTime.After(latestEnd) {
				latestEnd = target.EndTime
			}
		case "failed":
			failed++
			if earliestStart.IsZero() || target.StartTime.Before(earliestStart) {
				earliestStart = target.StartTime
			}
			if !target.EndTime.IsZero() && target.EndTime.After(latestEnd) {
				latestEnd = target.EndTime
			}
		case "running":
			running++
			if earliestStart.IsZero() || target.StartTime.Before(earliestStart) {
				earliestStart = target.StartTime
			}
		case "pending":
			pending++
		}
	}
	
	// Calculate total elapsed time
	var totalDuration time.Duration
	if !earliestStart.IsZero() {
		if running > 0 {
			// If tasks are still running, calculate from start to now
			totalDuration = time.Since(earliestStart)
		} else if !latestEnd.IsZero() {
			// All tasks completed, calculate from start to last end
			totalDuration = latestEnd.Sub(earliestStart)
		}
	}
	
	// Build progress bar
	total := len(m.targets)
	progressWidth := 30
	filledWidth := (completed * progressWidth) / total
	if filledWidth > progressWidth {
		filledWidth = progressWidth
	}
	
	progressBar := "["
	for i := 0; i < progressWidth; i++ {
		if i < filledWidth {
			progressBar += "="
		} else if i == filledWidth && running > 0 {
			progressBar += ">"
		} else {
			progressBar += " "
		}
	}
	progressBar += "]"
	
	// Add header with progress bar
	percentage := (completed * 100) / total
	header := fmt.Sprintf("%s %3d%% | %d/%d", progressBar, percentage, completed, total)
	
	// Add timing info
	if totalDuration > 0 {
		header += fmt.Sprintf(" | %s", totalDuration.Round(time.Millisecond))
	}
	
	// Add status counts
	if failed > 0 {
		header += fmt.Sprintf(" | FAILED: %d", failed)
	}
	
	lines = append(lines, header)
	lines = append(lines, "")

	// Show active targets (running/pending) in detail
	activeTargets := []string{}
	completedTargets := []string{}
	
	for _, target := range m.targets {
		// Format target line based on status
		var targetLine string
		spinner := m.getSpinner(target.Status)
		
		if target.Status == "running" || target.Status == "pending" {
			// Active target - show with current status
			targetLine = fmt.Sprintf("%s %s", spinner, target.Name)
			
			// Add most recent log message if available
			target.mu.RLock()
			if len(target.Logs) > 0 {
				lastLog := target.Logs[len(target.Logs)-1]
				targetLine += fmt.Sprintf(" - %s", lastLog.Message)
			}
			target.mu.RUnlock()
			
			activeTargets = append(activeTargets, targetLine)
		} else {
			// Completed/failed - show in compact form
			icon := m.getCompletionIcon(target.Status)
			duration := target.EndTime.Sub(target.StartTime).Round(time.Millisecond)
			targetLine = fmt.Sprintf("%s %s (%s)", icon, target.Name, duration)
			completedTargets = append(completedTargets, targetLine)
		}
	}
	
	// Show active targets first
	if len(activeTargets) > 0 {
		lines = append(lines, "Active:")
		for _, line := range activeTargets {
			lines = append(lines, "  "+line)
		}
		lines = append(lines, "")
	}
	
	// Show completed targets in compact form
	if len(completedTargets) > 0 {
		lines = append(lines, "Completed:")
		// Show up to 5 most recent completed, rest collapsed
		showCount := len(completedTargets)
		if showCount > 5 && m.height > 0 {
			showCount = 5
		}
		for i := 0; i < showCount; i++ {
			lines = append(lines, "  "+completedTargets[i])
		}
		if len(completedTargets) > showCount {
			lines = append(lines, fmt.Sprintf("  ... and %d more", len(completedTargets)-showCount))
		}
		// Add empty line after completed section
		lines = append(lines, "")
	}

	return strings.Join(lines, "\n")
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

func (m *Model) addLog(msg logMsg) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if msg.TargetIndex < 1 || msg.TargetIndex > len(m.targets) {
		return
	}

	target := m.targets[msg.TargetIndex-1] // Convert to 0-based index
	target.mu.Lock()
	defer target.mu.Unlock()

	target.Logs = append(target.Logs, LogEntry{
		Level:     msg.Level,
		Message:   msg.Message,
		Timestamp: time.Now(),
	})

	// Auto-update status on first log
	if target.Status == "pending" && msg.Level == "INFO" {
		target.Status = "running"
	}
}

func (m *Model) updateStatus(msg statusMsg) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if msg.TargetIndex < 1 || msg.TargetIndex > len(m.targets) {
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
	Level       string
	Message     string
}

type statusMsg struct {
	TargetIndex int
	Status      string
}