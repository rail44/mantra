package ai

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"strings"

	"github.com/ollama/ollama/api"
)

type Client struct {
	ollama *api.Client
	config *Config
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

	return &Client{
		ollama: ollamaClient,
		config: config,
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