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

	var sections []string
	
	// Count targets by status
	completed := 0
	failed := 0
	running := 0
	pending := 0
	for _, target := range m.targets {
		switch target.Status {
		case "completed":
			completed++
		case "failed":
			failed++
		case "running":
			running++
		case "pending":
			pending++
		}
	}
	
	// Add summary header
	summary := fmt.Sprintf("Progress: %d/%d completed", completed, len(m.targets))
	if running > 0 {
		summary += fmt.Sprintf(", %d running", running)
	}
	if pending > 0 {
		summary += fmt.Sprintf(", %d pending", pending)
	}
	if failed > 0 {
		summary += fmt.Sprintf(", %d failed", failed)
	}
	sections = append(sections, summary)
	sections = append(sections, strings.Repeat("=", 60))
	
	// Fixed height for each target (header + divider + 2 log lines = 4 lines total)
	logHeight := 2 // 2 lines for logs, plus header and divider makes 4 lines total

	for _, target := range m.targets {
		// Status icon
		icon := m.getStatusIcon(target.Status)
		
		// Header
		header := fmt.Sprintf("%s [%d/%d] %s", icon, target.Index, target.Total, target.Name)
		if target.Status == "completed" || target.Status == "failed" {
			duration := target.EndTime.Sub(target.StartTime).Round(time.Millisecond)
			header += fmt.Sprintf(" (%s)", duration)
		}
		
		// Check if this target should be collapsed
		isCollapsed := target.Status == "completed" || target.Status == "failed"
		
		// For collapsed targets, only show header
		if isCollapsed && m.height > 0 { // Only collapse in interactive mode
			sections = append(sections, header)
			continue
		}

		// Expanded view for active targets
		divider := strings.Repeat("-", 60)

		// Logs
		target.mu.RLock()
		logs := target.Logs
		// Only show recent 2 logs
		if len(logs) > logHeight {
			logs = logs[len(logs)-logHeight:]
		}
		
		var logLines []string
		for _, log := range logs {
			line := fmt.Sprintf("[%s] %-5s: %s",
				log.Timestamp.Format("15:04:05"),
				log.Level,
				log.Message)
			
			// Truncate long lines in interactive mode
			if m.height > 0 && len(line) > 80 {
				line = line[:77] + "..."
			}
			logLines = append(logLines, line)
		}
		target.mu.RUnlock()

		// Always pad to exactly 2 lines for consistent display
		for len(logLines) < logHeight {
			logLines = append(logLines, "")
		}

		// Build section
		section := header + "\n" +
			divider + "\n" +
			strings.Join(logLines, "\n")

		sections = append(sections, section)
	}

	return strings.Join(sections, "\n\n")
}

func (m *Model) getStatusIcon(status string) string {
	switch status {
	case "pending":
		return "[PENDING]"
	case "running":
		return "[RUNNING]"
	case "completed":
		return "[DONE]"
	case "failed":
		return "[FAILED]"
	default:
		return "[UNKNOWN]"
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