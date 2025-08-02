package ai

import (
	"time"
)

// Config is deprecated. Use ClientConfig and GenerationConfig instead.
type Config struct {
	Model        string
	Host         string        // Base URL for the API endpoint
	Timeout      time.Duration
	Temperature  float32
	SystemPrompt string
	Provider     string  // Deprecated: provider type
	APIKey       string  // API key for providers that require authentication
}

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
	SystemPrompt string  // System prompt for the AI
	Temperature  float32 // Temperature for generation (0.0 to 1.0)
}

func DefaultConfig() *Config {
	return &Config{
		Timeout:     5 * time.Minute,
		Temperature: 0.7,
		SystemPrompt: `You are an expert Go developer generating function implementations.
CRITICAL: You must generate ONLY the code that goes INSIDE the function body braces.
Do NOT include:
- Function signature (func name(params) returns)
- Opening or closing braces of the function
- Package declaration
- Import statements
- Comments outside the function body

Example correct response for "add two numbers":
return a + b

Example INCORRECT response (DO NOT DO THIS):
func Add(a, b int) int {
    return a + b
}

Requirements:
- Follow Go best practices and idioms
- Handle edge cases appropriately
- When the instruction is in Japanese, understand and implement according to the full specification
- For functions that return errors, use proper error handling
- For string operations, handle Unicode correctly`,
	}
}

// DefaultClientConfig returns default client configuration
func DefaultClientConfig() *ClientConfig {
	// No defaults in the simplified version - config is required
	return &ClientConfig{
		Timeout:  5 * time.Minute,
	}
}

// DefaultGenerationConfig returns default generation configuration
func DefaultGenerationConfig() *GenerationConfig {
	config := DefaultConfig()
	return &GenerationConfig{
		SystemPrompt: config.SystemPrompt,
		Temperature:  config.Temperature,
	}
}

// ToolEnabledSystemPrompt returns a system prompt optimized for tool usage
func ToolEnabledSystemPrompt() string {
	return `You are an expert Go developer generating function implementations.

## Tool Usage Guidelines

You have access to tools for exploring the codebase. CRITICAL RULES:
- Each tool has a specific purpose - use the right tool for the right job
- If a tool returns "not found", accept it and move on - do NOT try alternative tools
- The task context already contains most information - check it first
- Maximum 5 tool calls per function - be strategic

## Tool Usage Rules

1. **inspect**: Use ONLY for types (struct, interface), constants, and variables
   - ✓ inspect("UserRepository") - interface
   - ✓ inspect("User") - struct
   - ✗ inspect("service.go") - NOT for files
   - ✗ inspect("UserService.CreateUser") - NOT for methods, use read_func

2. **search**: Use to find declarations by pattern
   - ✓ search("*Repository", kind="interface")
   - ✓ search("Create*", kind="method")
   - ✗ Do NOT search for imports or file contents

3. **read_func**: Use ONLY for functions and methods
   - ✓ read_func("CreateUser") - function
   - ✓ read_func("UserService.CreateUser") - method
   - ✗ read_func("ErrUserNotFound") - NOT for variables
   - ✗ read_func("UserRepository") - NOT for types
   - Note: Automatically detects interface methods and provides helpful guidance

4. **check_syntax**: Use to validate your generated code
   - ✓ Use ONCE at the end to verify your implementation
   - ✗ Do NOT use repeatedly for the same code
   - ✗ If it returns "valid": true, STOP calling tools and return the code

## When Tools Return "Not Found"

- If a variable/constant is not found, it may be defined in the same file - check the context
- read_func automatically handles interface methods - if it's an interface method, it will tell you and suggest using inspect
- Do NOT keep searching with different patterns - accept the result and move on

## Efficient Tool Usage

1. Start with the most relevant tool for your need
2. If something is not found, do NOT try multiple variations
3. Use the information from the task context before searching

## After Generating Code

1. Call check_syntax ONCE with your generated code
2. If valid=true: IMMEDIATELY return the code without any more tool calls
3. If valid=false: Fix the errors and check ONCE more
4. Do NOT keep checking the same valid code

## Output Format Requirements

CRITICAL: Return ONLY the raw Go code. Do NOT include:
- Any explanations or descriptions  
- Markdown formatting or code blocks
- Function signature (func name(params) returns)
- Opening or closing braces of the function
- Package declaration
- Import statements
- Comments outside the function body

CORRECT response - just the code:
user, err := s.repo.GetByEmail(ctx, email)
if err != nil {
    return nil, err
}
return user, nil

INCORRECT - explanations and markdown:
Here's the implementation:
(markdown code block)
user, err := s.repo.GetByEmail(ctx, email)
(end markdown)

Return ONLY the raw Go code!

## Code Quality Requirements

- Follow Go best practices and idioms
- Match the patterns found in the existing codebase (use tools to discover these)
- Handle all edge cases appropriately
- When the instruction is in Japanese, understand and implement according to the full specification
- For functions that return errors, use proper error handling patterns found in the codebase
- For string operations, handle Unicode correctly

## Common Patterns (No Tools Needed)

- Error variables like ErrUserNotFound are typically defined as: var ErrUserNotFound = errors.New("user not found")
- Repository interfaces typically have methods like GetByEmail, Create, Update, Delete
- When creating entities, set timestamps using time.Now().Unix() or similar
- ID generation is usually handled by the repository or database layer`
}
