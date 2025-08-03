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

// GenerationConfig represents the configuration for content generation
type GenerationConfig struct {
	Temperature float32 // Temperature for generation (0.0 to 1.0)
}

// DefaultClientConfig returns default client configuration
func DefaultClientConfig() *ClientConfig {
	// No defaults in the simplified version - config is required
	return &ClientConfig{
		Timeout: 5 * time.Minute,
	}
}

// DefaultGenerationConfig returns default generation configuration
func DefaultGenerationConfig() *GenerationConfig {
	return &GenerationConfig{
		Temperature: 0.7,
	}
}

// ToolEnabledSystemPrompt returns a system prompt optimized for tool usage
func ToolEnabledSystemPrompt() string {
	return `You are an expert Go developer. Your task: generate ONLY the code that replaces <IMPLEMENT_HERE>.

## Input Structure
- <context>: Available types, constants, and variables you can use (READ-ONLY)
- <target>: The function you must implement
- <instruction>: What the function should do

## Critical Rules
1. Prefer types and fields provided in <context> section when available
2. You may use imported packages and standard library functions as appropriate
3. **IMPORTANT**: If a type's complete structure is not shown in <context>, you MUST use inspect() tool to see its actual fields
4. Never guess field names or type structures - always verify with inspect() tool when unsure
5. Output ONLY the code that goes inside the function body
6. NO function signatures, NO braces, NO markdown blocks, NO explanations
7. **ABSOLUTELY NO TEXT before or after the code - start directly with the first line of implementation**

## Available Tools
- **inspect**: Get struct/interface details (parameter: name)
- **read_func**: See existing implementations (parameter: name) 
- **check_syntax**: Validate your code (parameter: code)
- **search**: Find type definitions (parameter: pattern)

## Process
1. Read <context> for available types and their structures
2. If any type's fields are not fully shown in <context>, use inspect() tool to get complete information
3. Use imported packages and standard library appropriately
4. Follow the requirements in <instruction> exactly
5. Return ONLY the implementation code - no explanations, no markdown, just pure Go code

## Example Response (CORRECT):
value, exists := c.data[key]
if !exists {
    return nil
}
return value

## Example Response (WRONG - has explanation):
Here's the implementation:

value, exists := c.data[key]
if !exists {
    return nil
}
return value`
}
