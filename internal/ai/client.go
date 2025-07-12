package ai

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"strings"

	"github.com/ollama/ollama/api"
	"github.com/rail44/glyph/internal/modelfile"
	"github.com/rail44/glyph/internal/parser"
)

type Client struct {
	ollama   *api.Client
	config   *Config
	renderer *modelfile.Renderer
}

func NewClient(config *Config) (*Client, error) {
	if config == nil {
		config = DefaultConfig()
	}

	// Create Ollama client
	var ollamaClient *api.Client
	
	if config.Host != "" {
		hostURL, err := url.Parse(config.Host)
		if err != nil {
			return nil, fmt.Errorf("invalid host URL: %w", err)
		}
		ollamaClient = api.NewClient(hostURL, http.DefaultClient)
	} else {
		// Use default host
		defaultURL, _ := url.Parse("http://localhost:11434")
		ollamaClient = api.NewClient(defaultURL, http.DefaultClient)
	}

	// Create modelfile renderer
	mode := config.Mode
	if mode == "" {
		mode = "spanner" // Default mode
	}
	renderer := modelfile.NewRenderer(mode)

	return &Client{
		ollama:   ollamaClient,
		config:   config,
		renderer: renderer,
	}, nil
}

// Generate sends a prompt to the AI and returns the response
func (c *Client) Generate(ctx context.Context, prompt string) (string, error) {
	messages := []api.Message{
		{
			Role:    "system",
			Content: c.config.SystemPrompt,
		},
		{
			Role:    "user",
			Content: prompt,
		},
	}

	var response strings.Builder
	
	err := c.ollama.Chat(ctx, &api.ChatRequest{
		Model:    c.config.Model,
		Messages: messages,
		Options: map[string]interface{}{
			"temperature": c.config.Temperature,
		},
	}, func(resp api.ChatResponse) error {
		response.WriteString(resp.Message.Content)
		return nil
	})

	if err != nil {
		return "", fmt.Errorf("chat failed: %w", err)
	}

	return response.String(), nil
}

// GenerateStream sends a prompt and returns a channel for streaming responses
func (c *Client) GenerateStream(ctx context.Context, prompt string) (<-chan string, <-chan error) {
	messages := []api.Message{
		{
			Role:    "system",
			Content: c.config.SystemPrompt,
		},
		{
			Role:    "user",
			Content: prompt,
		},
	}

	outputCh := make(chan string, 100)
	errorCh := make(chan error, 1)

	go func() {
		defer close(outputCh)
		defer close(errorCh)

		err := c.ollama.Chat(ctx, &api.ChatRequest{
			Model:    c.config.Model,
			Messages: messages,
			Options: map[string]interface{}{
				"temperature": c.config.Temperature,
			},
		}, func(resp api.ChatResponse) error {
			select {
			case outputCh <- resp.Message.Content:
			case <-ctx.Done():
				return ctx.Err()
			}
			return nil
		})

		if err != nil {
			errorCh <- err
		}
	}()

	return outputCh, errorCh
}

// GenerateWithDeclaration generates code using mode-specific Modelfile
func (c *Client) GenerateWithDeclaration(ctx context.Context, prompt string, decl *parser.Declaration) (string, error) {
	// Create temporary Modelfile
	modelfilePath, err := c.renderer.CreateTempModelfile(decl)
	if err != nil {
		return "", fmt.Errorf("failed to create modelfile: %w", err)
	}
	defer os.Remove(modelfilePath)

	// Create a unique model name for this session
	modelName := fmt.Sprintf("glyph-temp-%d", os.Getpid())
	
	// Create the model using ollama
	if err := c.createModel(modelName, modelfilePath); err != nil {
		return "", fmt.Errorf("failed to create model: %w", err)
	}
	defer c.deleteModel(modelName)

	// Generate using the custom model
	return c.generateWithModel(ctx, modelName, prompt)
}

func (c *Client) createModel(name, modelfilePath string) error {
	cmd := exec.Command("ollama", "create", name, "-f", modelfilePath)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("ollama create failed: %w\nOutput: %s", err, string(output))
	}
	return nil
}

func (c *Client) deleteModel(name string) error {
	cmd := exec.Command("ollama", "rm", name)
	// Ignore errors as this is cleanup
	cmd.Run()
	return nil
}

func (c *Client) generateWithModel(ctx context.Context, modelName, prompt string) (string, error) {
	var response strings.Builder
	
	err := c.ollama.Generate(ctx, &api.GenerateRequest{
		Model:  modelName,
		Prompt: prompt,
		Stream: new(bool), // Disable streaming for simplicity
	}, func(resp api.GenerateResponse) error {
		response.WriteString(resp.Response)
		return nil
	})

	if err != nil {
		return "", fmt.Errorf("generation failed: %w", err)
	}

	return response.String(), nil
}

// CheckModel verifies if the specified model is available
func (c *Client) CheckModel(ctx context.Context) error {
	_, err := c.ollama.Show(ctx, &api.ShowRequest{
		Model: c.config.Model,
	})
	
	if err != nil {
		return fmt.Errorf("model %s not found: %w", c.config.Model, err)
	}

	return nil
}