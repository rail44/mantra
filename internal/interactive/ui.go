package interactive

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/rail44/glyph/internal/ai"
	"github.com/rail44/glyph/internal/generator"
	"github.com/rail44/glyph/internal/parser"
	"github.com/rail44/glyph/internal/prompt"
)

type status int

const (
	statusWatching status = iota
	statusParsing
	statusGenerating
	statusWriting
	statusError
	statusSuccess
)

type model struct {
	filePath   string
	status     status
	lastUpdate time.Time
	error      error
	aiClient   *ai.Client
	generator  *generator.Generator
	
	// UI state
	spinner    int
	width      int
	height     int
}

type fileChangedMsg struct{}
type generationCompleteMsg struct{ err error }
type tickMsg time.Time

func NewModel(filePath string, aiClient *ai.Client) model {
	return model{
		filePath:  filePath,
		status:    statusWatching,
		aiClient:  aiClient,
		generator: generator.New(filePath),
	}
}

func (m model) Init() tea.Cmd {
	return tea.Batch(
		tick(),
		m.checkModel(),
	)
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil

	case tea.KeyMsg:
		if msg.String() == "q" || msg.String() == "ctrl+c" {
			return m, tea.Quit
		}

	case fileChangedMsg:
		m.status = statusParsing
		m.lastUpdate = time.Now()
		return m, m.regenerate()

	case generationCompleteMsg:
		if msg.err != nil {
			m.status = statusError
			m.error = msg.err
		} else {
			m.status = statusSuccess
		}
		return m, nil

	case tickMsg:
		if m.status == statusGenerating {
			m.spinner++
		}
		return m, tick()
	}

	return m, nil
}

func (m model) View() string {
	var s strings.Builder

	// Header
	headerStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("12")).
		MarginBottom(1)
	
	s.WriteString(headerStyle.Render("🔮 Glyph - AI Code Generator"))
	s.WriteString("\n\n")

	// File info
	fileStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("241"))
	s.WriteString(fileStyle.Render(fmt.Sprintf("Watching: %s", m.filePath)))
	s.WriteString("\n")
	s.WriteString(fileStyle.Render(fmt.Sprintf("Output:   %s", m.generator.GetOutputPath())))
	s.WriteString("\n\n")

	// Status
	statusStyle := lipgloss.NewStyle().Bold(true)
	switch m.status {
	case statusWatching:
		s.WriteString(statusStyle.Foreground(lipgloss.Color("10")).Render("✓ Watching for changes..."))
	case statusParsing:
		s.WriteString(statusStyle.Foreground(lipgloss.Color("11")).Render("📖 Parsing declaration..."))
	case statusGenerating:
		spinner := []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}
		s.WriteString(statusStyle.Foreground(lipgloss.Color("12")).Render(
			fmt.Sprintf("%s Generating implementation...", spinner[m.spinner%len(spinner)])))
	case statusWriting:
		s.WriteString(statusStyle.Foreground(lipgloss.Color("12")).Render("💾 Writing file..."))
	case statusSuccess:
		s.WriteString(statusStyle.Foreground(lipgloss.Color("10")).Render("✓ Implementation generated successfully!"))
		if !m.lastUpdate.IsZero() {
			s.WriteString(fmt.Sprintf(" (%s)", time.Since(m.lastUpdate).Round(time.Millisecond)))
		}
	case statusError:
		s.WriteString(statusStyle.Foreground(lipgloss.Color("9")).Render("✗ Error: "))
		if m.error != nil {
			s.WriteString(m.error.Error())
		}
	}
	s.WriteString("\n\n")

	// Instructions
	helpStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("241"))
	s.WriteString(helpStyle.Render("Press 'q' to quit"))

	return s.String()
}

func (m model) regenerate() tea.Cmd {
	return func() tea.Msg {
		ctx := context.Background()
		
		// Parse the declaration
		decl, err := parser.ParseFile(m.filePath)
		if err != nil {
			return generationCompleteMsg{err: fmt.Errorf("parse error: %w", err)}
		}

		// Gather context
		context, err := prompt.GatherContext(m.filePath)
		if err != nil {
			return generationCompleteMsg{err: fmt.Errorf("context error: %w", err)}
		}

		// Build prompt
		builder := prompt.NewBuilder(context)
		fullPrompt := builder.Build(decl)

		// Generate implementation
		response, err := m.aiClient.Generate(ctx, fullPrompt)
		if err != nil {
			return generationCompleteMsg{err: fmt.Errorf("generation error: %w", err)}
		}

		// Write the file
		err = m.generator.Generate(response)
		if err != nil {
			return generationCompleteMsg{err: fmt.Errorf("write error: %w", err)}
		}

		return generationCompleteMsg{err: nil}
	}
}

func (m model) checkModel() tea.Cmd {
	return func() tea.Msg {
		ctx := context.Background()
		if err := m.aiClient.CheckModel(ctx); err != nil {
			return generationCompleteMsg{err: fmt.Errorf("model check failed: %w", err)}
		}
		return nil
	}
}

func tick() tea.Cmd {
	return tea.Tick(100*time.Millisecond, func(t time.Time) tea.Msg {
		return tickMsg(t)
	})
}

// FileChanged returns a command that triggers regeneration
func FileChanged() tea.Msg {
	return fileChangedMsg{}
}