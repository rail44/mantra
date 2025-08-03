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

Mantra uses a two-phase approach for intelligent code generation:

### Phase 1: Context Gathering (Temperature: 0.6)
1. **Comment Detection**: Finds functions marked with `// mantra:` comments
2. **Dynamic Exploration**: AI actively explores your codebase using tools:
   - `search`: Finds relevant types and patterns
   - `inspect`: Gets detailed struct/interface definitions
   - `read_func`: Examines existing function implementations
3. **Context Building**: Gathers all necessary type definitions, functions, and imports

### Phase 2: Implementation (Temperature: 0.2)
1. **Focused Generation**: Uses gathered context to generate precise implementations
2. **Syntax Validation**: AI validates generated code with `check_syntax` tool
3. **Code Writing**: Produces clean, idiomatic Go code
4. **Format & Save**: Formats and saves to the configured output directory

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
model = "qwen2.5-coder:32b"
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
model = "mistralai/mistral-large-latest"
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

## Two-Phase Architecture

Mantra employs a sophisticated two-phase approach to ensure accurate and context-aware code generation:

### Phase 1: Context Gathering
- **Higher Temperature (0.6)**: Encourages exploration and discovery
- **Available Tools**:
  - `search`: Find type definitions and patterns in the codebase
  - `inspect`: Get detailed struct/interface information
  - `read_func`: Examine existing function implementations
- **Output**: Structured context with types, functions, constants, and imports

### Phase 2: Implementation
- **Lower Temperature (0.2)**: Ensures precise, deterministic code generation
- **Available Tools**:
  - `check_syntax`: Validate generated Go code before returning
- **Input**: Original prompt enhanced with discovered context
- **Output**: Clean, working Go implementation

This architecture ensures that the AI has all necessary information before generating code, resulting in more accurate and complete implementations.

## Performance Features

### Phase-Based Optimization
- **Parallel Processing**: Multiple targets can be processed concurrently
- **Smart Caching**: Project root detection is cached per file batch
- **Efficient Tool Usage**: Tools are loaded only for the phases that need them

### Real-time Feedback
- Progress indicators for each phase
- Detailed timing information for performance analysis
- Clear error messages if generation fails

### Performance Monitoring
Use debug or trace log levels to analyze performance:
```bash
mantra generate . --log-level debug
```

This reveals:
- Context gathering time vs implementation time
- Tool execution metrics
- API call timings
- Overall generation performance

## Best Practices

1. **Clear Instructions**: Be specific about what you want
2. **Context Matters**: Include relevant types and imports in your file
3. **One Thing at a Time**: Focus each function on a single responsibility
4. **Review Generated Code**: Always review and test generated implementations
5. **Use Streaming**: Watch progress to catch issues early

## Examples

See the `examples/` directory for more usage examples:
- `examples/simple/`: Basic cache implementation
- `examples/complex/`: Advanced business logic with order management, user repositories, and caching services

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