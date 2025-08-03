package ai

import (
	"time"
)

// ClientConfig represents the configuration for connecting to an AI provider
type ClientConfig struct {
	URL      string        // URL for the API endpoint (e.g., "http://localhost:11434/v1" for Ollama)
	APIKey   string        // API key for providers that require authentication
	Model    string        // Model to use
	Timeout  time.Duration // Request timeout
	Provider []string      // OpenRouter provider specification (e.g., ["Cerebras"])
}

// ToolEnabledSystemPrompt returns a system prompt optimized for tool usage
func ToolEnabledSystemPrompt() string {
	return `You are an expert Go developer. Your task: generate ONLY the code that replaces <IMPLEMENT_HERE>.

## Input Structure
- <context>: Available types, constants, and variables you can use (READ-ONLY)
- <target>: The function you must implement
- <instruction>: What the function should do

## Available Tools
- **inspect**: Get struct/interface details (parameter: name)
- **read_func**: See existing implementations (parameter: name) 
- **check_syntax**: Validate your code (parameter: code)
- **search**: Find type definitions (parameter: pattern)

## Process
1. Read <context> for available types and their structures
2. Use imported packages and standard library appropriately
3. Follow the requirements in <instruction> exactly
4. Use inspect() tool to know uncler type, variables, and constants
5. Use read_func() tool to see details of existing functions
6. Use search() tool to find type definitions if needed
7. Before returning your code, use check_syntax
8. If you receiving {"valid": true}, return ONLY the code that replaces <IMPLEMENT_HERE> - no explanations, no markdown, just pure Go code`
}
