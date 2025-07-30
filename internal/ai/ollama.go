package ai

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/ollama/ollama/api"
)

// OllamaClient implements Provider for Ollama
type OllamaClient struct {
	client      *api.Client
	model       string
	temperature float32
	systemPrompt string
	debugTiming bool
}

// NewOllamaClient creates a new Ollama client
func NewOllamaClient(host, model string, temperature float32, systemPrompt string) (*OllamaClient, error) {
	var ollamaClient *api.Client

	if host != "" {
		hostURL, err := url.Parse(host)
		if err != nil {
			return nil, fmt.Errorf("invalid host URL: %w", err)
		}
		ollamaClient = api.NewClient(hostURL, http.DefaultClient)
	} else {
		// Use default host
		defaultURL, _ := url.Parse("http://localhost:11434")
		ollamaClient = api.NewClient(defaultURL, http.DefaultClient)
	}

	return &OllamaClient{
		client:      ollamaClient,
		model:       model,
		temperature: temperature,
		systemPrompt: systemPrompt,
	}, nil
}

// SetDebugTiming enables detailed timing information
func (c *OllamaClient) SetDebugTiming(enabled bool) {
	c.debugTiming = enabled
}

// Name returns the provider name
func (c *OllamaClient) Name() string {
	return "Ollama"
}

// Generate sends a prompt to the AI and returns the response
func (c *OllamaClient) Generate(ctx context.Context, prompt string) (string, error) {
	totalStart := time.Now()

	// Build messages
	buildStart := time.Now()
	messages := []api.Message{
		{
			Role:    "system",
			Content: c.systemPrompt,
		},
		{
			Role:    "user",
			Content: prompt,
		},
	}
	if c.debugTiming {
		fmt.Printf("    [AI Timing] Message building: %v\n", time.Since(buildStart))
		fmt.Printf("    [AI Timing] Prompt size: %d chars\n", len(prompt))
	}

	var response strings.Builder
	firstTokenTime := time.Duration(0)
	tokenCount := 0

	// Make the API call
	apiCallStart := time.Now()
	err := c.client.Chat(ctx, &api.ChatRequest{
		Model:    c.model,
		Messages: messages,
		Options: map[string]interface{}{
			"temperature": c.temperature,
		},
	}, func(resp api.ChatResponse) error {
		if firstTokenTime == 0 {
			firstTokenTime = time.Since(apiCallStart)
			if c.debugTiming {
				fmt.Printf("    [AI Timing] First token received: %v\n", firstTokenTime)
			}
		}
		tokenCount++
		response.WriteString(resp.Message.Content)
		return nil
	})

	totalTime := time.Since(totalStart)

	if c.debugTiming {
		fmt.Printf("    [AI Timing] Total API call: %v\n", time.Since(apiCallStart))
		fmt.Printf("    [AI Timing] Tokens received: %d\n", tokenCount)
		fmt.Printf("    [AI Timing] Response size: %d chars\n", response.Len())
		if tokenCount > 0 && totalTime > 0 {
			tokensPerSecond := float64(tokenCount) / totalTime.Seconds()
			fmt.Printf("    [AI Timing] Throughput: %.1f tokens/sec\n", tokensPerSecond)
		}
	}

	if err != nil {
		return "", fmt.Errorf("chat failed: %w", err)
	}

	return response.String(), nil
}

// GenerateStream sends a prompt and returns a channel for streaming responses
func (c *OllamaClient) GenerateStream(ctx context.Context, prompt string) (<-chan string, <-chan error) {
	// Build messages
	messages := []api.Message{
		{
			Role:    "system",
			Content: c.systemPrompt,
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

		err := c.client.Chat(ctx, &api.ChatRequest{
			Model:    c.model,
			Messages: messages,
			Options: map[string]interface{}{
				"temperature": c.temperature,
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
func (c *OllamaClient) CheckModel(ctx context.Context) error {
	checkStart := time.Now()

	_, err := c.client.Show(ctx, &api.ShowRequest{
		Model: c.model,
	})

	if c.debugTiming {
		fmt.Printf("    [AI Timing] Model check: %v\n", time.Since(checkStart))
	}

	if err != nil {
		return fmt.Errorf("model %s not found: %w", c.model, err)
	}

	return nil
}