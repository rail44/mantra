# 🔮 Mantra

> **⚠️ Work in Progress (WIP)**: This project is under active development and is not suitable for production use. APIs, features, and generated code quality may change significantly without notice. Use at your own risk for experimentation and development purposes only.

Mantra is a local-first AI-powered Go code generation tool that transforms natural language instructions into working implementations.

## Features

- **Natural Language Programming**: Describe what you want in plain language
- **Safe Generation**: Generates implementations to separate files, preserving original source
- **Context-Aware**: Understands your function signatures and surrounding code
- **Smart Examples**: Learns from previously generated code to maintain consistency
- **Flexible AI Backend**: Works with Ollama (local), OpenAI, deepinfra, or any OpenAI-compatible API
- **Real-time Streaming**: See generation progress as it happens
- **Flexible Output**: Generate to separate files or replace in-place

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
- One of the following AI backends:
  - [Ollama](https://ollama.ai/) installed and running locally (default)
  - OpenAI API key
  - deepinfra API key
  - Any OpenAI-compatible API endpoint

## Quick Start

1. Write your Go code with `// mantra:` comments:

```go
package main

import "context"

// mantra: emailでユーザーを検索
func GetUserByEmail(ctx context.Context, email string) (*User, error) {
    panic("not implemented")
}

// mantra: 割引率を計算する
// 購入金額が10000円以上で10%割引
// 会員ランクがGoldなら追加5%割引
func CalculateDiscount(amount float64, memberRank string) float64 {
    panic("not implemented")
}
```

2. Generate implementations:

```bash
mantra generate main.go
```

3. Watch the streaming progress as code is generated in real-time!

## How It Works

1. **Comment Detection**: Mantra finds functions marked with `// mantra:` comments
2. **Context Analysis**: Analyzes function signatures and surrounding code
3. **Example Learning**: Learns from previously generated implementations to maintain consistency
4. **AI Generation**: Sends context and instructions to your AI model
5. **Tool Usage** (Optional): When enabled with `--use-tools`, the AI can dynamically inspect code
6. **Real-time Streaming**: Shows generation progress with live feedback
7. **Code Generation**: Generates implementations based on your output preference
8. **Format & Save**: Formats the code and saves to target location

### Output Modes

#### Safe Separate File Generation (Default)
- Preserves original source files unchanged
- Generates implementations in separate package
- Methods become functions with receiver as first parameter
- Benefits:
  - Source code protection
  - Easy code review and validation
  - Gradual integration workflow
  - Version control friendly

## Configuration

Mantra uses a TOML configuration file (`mantra.toml`) that should be placed in your project root or package directory. The tool searches for the configuration file starting from the target directory and moving up the directory tree.

### Basic Configuration

Create a `mantra.toml` file:

```toml
# Model to use for code generation (required)
model = "devstral"

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

**Ollama (Local):**
```toml
model = "devstral"
url = "http://localhost:11434/v1"
dest = "./generated"
```

**OpenAI:**
```toml
model = "gpt-4"
url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"
dest = "./generated"
```

**deepinfra:**
```toml
model = "mistralai/Devstral-Small-2507"
url = "https://api.deepinfra.com/v1/openai"
api_key = "${DEEPINFRA_API_KEY}"
dest = "./generated"
```

**OpenRouter:**
```toml
model = "anthropic/claude-3-sonnet"
url = "https://openrouter.ai/api/v1"
api_key = "${OPENROUTER_API_KEY}"
dest = "./generated"

[openrouter]
providers = ["Cerebras"]  # Route to specific providers
```

### Environment Variable Expansion

The configuration supports environment variable expansion using `${VAR_NAME}` syntax. This is particularly useful for API keys:

```toml
api_key = "${OPENAI_API_KEY}"
```

Set the environment variable before running Mantra:
```bash
export OPENAI_API_KEY="your-api-key"
mantra generate .
```

## Commands

### Generate
```bash
mantra generate [package-dir]
```

Generates implementations for all functions with `// mantra:` comments in the specified package directory (defaults to current directory).

The command:
1. Searches for `mantra.toml` configuration file starting from the package directory and moving up
2. Detects all functions marked with `// mantra:` comments
3. Checks which implementations are new or outdated
4. Generates code for pending targets
5. Writes generated files to the configured output directory

**Examples:**
```bash
# Generate for current directory
mantra generate

# Generate for specific package
mantra generate ./pkg/user

# Generate with custom config location
cd myproject && mantra generate ./internal/service
```

## Writing Effective Instructions

### Basic Instructions
```go
// mantra: IDでユーザーを取得
func GetUser(id string) (*User, error) {
    panic("not implemented")
}
```

### Detailed Instructions
```go
// mantra: ユーザー認証を実行
// - パスワードをbcryptで検証
// - 成功時はJWTトークンを生成
// - 失敗回数をRedisでカウント
func AuthenticateUser(email, password string) (*Token, error) {
    panic("not implemented")
}
```

### Method Generation
```go
type UserService struct {
    db *sql.DB
}

// mantra: データベースから全ユーザーを取得
func (s *UserService) GetAllUsers(ctx context.Context) ([]*User, error) {
    panic("not implemented")
}
```

## Code Generation

Mantra generates clean, idiomatic Go code with:
- Parameterized queries for database operations
- Proper error handling and context usage
- Best practices for the detected use case
- Comprehensive implementations based on your instructions

## Tool System (Currently Disabled)

Note: Tool support is temporarily disabled in the simplified version. The AI generates code based on the provided context without dynamic inspection capabilities.

### Available Tools

1. **inspect**: Get detailed information about any Go declaration (structs, interfaces, functions, etc.)
2. **search**: Search for declarations using pattern matching
3. **read_func**: Read the implementation of functions and methods
4. **check_syntax**: Validate Go syntax before generating code

### Benefits

- **Better Understanding**: AI can explore your codebase dynamically
- **Higher Accuracy**: AI verifies types and interfaces before using them
- **Smarter Generation**: AI learns from existing patterns in your code

### Usage

```bash
# Enable tools for more accurate generation
mantra generate --use-tools main.go
```

## Performance Features

### Streaming Output
By default, Mantra shows real-time progress as AI generates your code:
- See dots appear as tokens are generated
- Get immediate feedback that generation is working
- Cancel if generation seems to be going wrong

### Optimized Prompts
Mantra automatically chooses the right prompt complexity:
- **Simple functions**: Minimal prompts for faster generation
- **Complex functions**: Detailed prompts with full context

### Performance Analysis
Use debug or trace log levels to identify performance bottlenecks:
```bash
mantra generate main.go --log-level debug
```

This shows:
- Time spent parsing
- AI model loading time
- Generation time per function
- Total execution time

## Best Practices

1. **Clear Instructions**: Be specific about what you want
2. **Context Matters**: Include relevant types and imports in your file
3. **One Thing at a Time**: Focus each function on a single responsibility
4. **Review Generated Code**: Always review and test generated implementations
5. **Use Streaming**: Watch progress to catch issues early

## Examples

See the `examples/` directory for more usage examples:
- `user_service.go`: Database operations example (using Spanner)
- `calculator.go`: General computation functions

## Troubleshooting

### Slow Generation
- Use `--log-level debug` to identify bottlenecks
- Try simpler, more focused instructions
- Consider using a smaller/faster model

### Generation Stuck
- The streaming output shows if generation is progressing
- Cancel with Ctrl+C if needed
- Check that Ollama is running: `ollama list`

## License

MIT License - See LICENSE file for details