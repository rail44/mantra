# 🔮 Glyph

Glyph is a local-first AI-powered Go code generation tool that transforms natural language instructions into working implementations.

## Features

- **Natural Language Programming**: Describe what you want in plain language
- **Safe Generation**: Generates implementations to separate files, preserving original source
- **Context-Aware**: Understands your function signatures and surrounding code
- **Local-First**: Everything runs on your machine with Ollama
- **Real-time Streaming**: See generation progress as it happens
- **Flexible Output**: Generate to separate files or replace in-place

## Installation

```bash
go install github.com/rail44/glyph@latest
```

Or build from source:

```bash
git clone https://github.com/rail44/glyph.git
cd glyph
go build -o glyph .
```

## Prerequisites

- Go 1.21 or later
- [Ollama](https://ollama.ai/) installed and running
- A compatible AI model (e.g., `devstral`)

## Quick Start

1. Write your Go code with `// glyph:` comments:

```go
package main

import "context"

// glyph: emailでユーザーを検索
func GetUserByEmail(ctx context.Context, email string) (*User, error) {
    panic("not implemented")
}

// glyph: 割引率を計算する
// 購入金額が10000円以上で10%割引
// 会員ランクがGoldなら追加5%割引
func CalculateDiscount(amount float64, memberRank string) float64 {
    panic("not implemented")
}
```

2. Generate implementations:

```bash
glyph generate main.go
```

3. Watch the streaming progress as code is generated in real-time!

## How It Works

1. **Comment Detection**: Glyph finds functions marked with `// glyph:` comments
2. **Context Analysis**: Analyzes function signatures and surrounding code
3. **AI Generation**: Sends context and instructions to your local AI model
4. **Real-time Streaming**: Shows generation progress with live feedback
5. **Code Generation**: Generates implementations based on your output preference
6. **Format & Save**: Formats the code and saves to target location

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

Configuration is handled via command-line flags:

```bash
# Use different model
glyph generate --model qwen2.5-coder main.go

# Use different Ollama host
glyph generate --host http://192.168.1.100:11434 main.go
```

Defaults:
- Model: `devstral`
- Host: `http://localhost:11434`

## Commands

### Generate
```bash
glyph generate <file> [flags]
```

Generates implementations for all functions with `// glyph:` comments.

**Flags:**
- `--model string`: AI model to use (default: `devstral`)
- `--host string`: Ollama host URL (default: `http://localhost:11434`)
- `--no-stream`: Disable streaming output (faster for scripting)
- `--debug-timing`: Show detailed timing information
- `--output-dir string`: Directory for generated files (default: `./generated`)
- `--package-name string`: Package name for generated files (default: `generated`)

### Output Options

```bash
# Default: generate to separate files (preserves original source)
glyph generate main.go

# Generate to custom directory and package
glyph generate main.go --output-dir ./impl --package-name impl
```

### Performance Options

```bash
# Default: streaming with progress indication
glyph generate main.go

# Non-streaming for scripting/CI
glyph generate main.go --no-stream

# Debug performance issues
glyph generate main.go --debug-timing
```

## Writing Effective Instructions

### Basic Instructions
```go
// glyph: IDでユーザーを取得
func GetUser(id string) (*User, error) {
    panic("not implemented")
}
```

### Detailed Instructions
```go
// glyph: ユーザー認証を実行
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

// glyph: データベースから全ユーザーを取得
func (s *UserService) GetAllUsers(ctx context.Context) ([]*User, error) {
    panic("not implemented")
}
```

## Code Generation

Glyph generates clean, idiomatic Go code with:
- Parameterized queries for database operations
- Proper error handling and context usage
- Best practices for the detected use case
- Comprehensive implementations based on your instructions

## Performance Features

### Streaming Output
By default, Glyph shows real-time progress as AI generates your code:
- See dots appear as tokens are generated
- Get immediate feedback that generation is working
- Cancel if generation seems to be going wrong

### Optimized Prompts
Glyph automatically chooses the right prompt complexity:
- **Simple functions**: Minimal prompts for faster generation
- **Complex functions**: Detailed prompts with full context

### Timing Analysis
Use `--debug-timing` to identify performance bottlenecks:
```bash
glyph generate main.go --debug-timing
```

This shows:
- Time spent parsing
- AI model loading time
- First token latency
- Total generation time
- Tokens per second

## Best Practices

1. **Clear Instructions**: Be specific about what you want
2. **Context Matters**: Include relevant types and imports in your file
3. **One Thing at a Time**: Focus each function on a single responsibility
4. **Review Generated Code**: Always review and test generated implementations
5. **Use Streaming**: Watch progress to catch issues early

## Examples

See the `examples/` directory for more usage examples:
- `user_service.go`: Database operations with Spanner
- `calculator.go`: General computation functions

## Troubleshooting

### Slow Generation
- Use `--debug-timing` to identify bottlenecks
- Try simpler, more focused instructions
- Consider using a smaller/faster model

### Generation Stuck
- The streaming output shows if generation is progressing
- Cancel with Ctrl+C if needed
- Check that Ollama is running: `ollama list`

## License

MIT License - See LICENSE file for details