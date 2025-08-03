# 🔮 Mantra

> **⚠️ Work in Progress (WIP)**: This project is under active development and is not suitable for production use. APIs, features, and generated code quality may change significantly without notice. Use at your own risk for experimentation and development purposes only.

Mantra is a local-first AI-powered Go code generation tool that transforms natural language instructions into working implementations.

## Features

- **Natural Language Programming**: Describe what you want in plain language
- **Two-Phase Generation**: Context gathering followed by implementation
- **Safe by Default**: Never modifies original source files
- **Context-Aware**: Understands your types, functions, and project structure
- **Multiple AI Providers**: Works with Ollama (local), OpenAI, OpenRouter, and others

## Installation

```bash
go install github.com/rail44/mantra@latest
```

Or build from source:

```bash
git clone https://github.com/rail44/mantra.git
cd mantra
go build -o mantra .
```

## Prerequisites

- Go 1.21 or later
- Any OpenAI-compatible API endpoint

## Quick Start

1. Write your Go code with `// mantra:` comments:

```go
package main

import "context"

// mantra: Get user by email from database
func GetUserByEmail(ctx context.Context, email string) (*User, error) {
    panic("not implemented")
}

// mantra: Calculate discount based on amount and member rank
// - 10% off for purchases over $100
// - Additional 5% for Gold members
func CalculateDiscount(amount float64, memberRank string) float64 {
    panic("not implemented")
}
```

2. Generate implementations:

```bash
mantra generate .
```

3. Check the generated code in your configured output directory.

## How It Works

Mantra uses a two-phase approach:

1. **Context Gathering**: AI explores your codebase to understand types and patterns
2. **Implementation**: Generates code using the gathered context

Generated code is saved to a separate directory, keeping your source files unchanged.

## Configuration

Create a `mantra.toml` file in your project:

```toml
# Model to use for code generation (required)
model = "qwen2.5-coder:32b"

# API endpoint URL (required)
url = "http://localhost:11434/v1"

# Output directory for generated files (required)
# Package name will be derived from the directory name
dest = "./generated"

# API key for authentication (optional)
# Supports environment variable expansion
api_key = "${OPENAI_API_KEY}"

# Log level: error, warn, info, debug, trace
log_level = "info"
```

### Provider Examples

<details>
<summary>Ollama (Local)</summary>

```toml
model = "qwen2.5-coder:32b"
url = "http://localhost:11434/v1"
dest = "./generated"
```
</details>

<details>
<summary>OpenAI</summary>

```toml
model = "gpt-4"
url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"
dest = "./generated"
```
</details>

<details>
<summary>OpenRouter</summary>

```toml
model = "anthropic/claude-3-sonnet"
url = "https://openrouter.ai/api/v1"
api_key = "${OPENROUTER_API_KEY}"
dest = "./generated"

[openrouter]
providers = ["Cerebras"]  # Optional: route to specific providers
```
</details>

## Usage

```bash
mantra generate [package-dir]
```

Generates implementations for all functions with `// mantra:` comments.

```bash
# Current directory
mantra generate

# Specific package
mantra generate ./pkg/user
```

## Writing Instructions

### Simple
```go
// mantra: Get user by ID from database
func GetUser(id string) (*User, error) {
    panic("not implemented")
}
```

### Detailed
```go
// mantra: Authenticate user
// - Verify password with bcrypt
// - Generate JWT token on success
// - Track failed attempts in Redis
func AuthenticateUser(email, password string) (*Token, error) {
    panic("not implemented")
}
```

### Methods
```go
type UserService struct {
    db *sql.DB
}

// mantra: Get all users from database
func (s *UserService) GetAllUsers(ctx context.Context) ([]*User, error) {
    panic("not implemented")
}
```



## Logging and Debugging

Use different log levels to see what's happening:

```bash
# See detailed progress
mantra generate . --log-level debug

# See tool execution details
mantra generate . --log-level trace
```

## Best Practices

1. **Clear Instructions**: Be specific about what you want
2. **Context Matters**: Include relevant types and imports in your file
3. **One Thing at a Time**: Focus each function on a single responsibility
4. **Review Generated Code**: Always review and test generated implementations

## Examples

See the `examples/` directory for more usage examples:
- `examples/simple/`: Basic cache implementation
- `examples/complex/`: Advanced business logic with order management, user repositories, and caching services

## Troubleshooting

### Slow Generation
- Use `--log-level debug` to identify bottlenecks
- Try simpler, more focused instructions
- Consider using a smaller/faster model

### Generation Issues
- Check that your AI backend is running (e.g., `ollama list`)
- Verify your API key is set correctly
- Review error messages in debug mode

## License

MIT License - See LICENSE file for details
