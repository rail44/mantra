# 🔮 Glyph

Glyph is a local-first interactive development tool that generates AI-powered Spanner-optimized Go data access layer code from declarative specifications.

## Features

- **Declarative Programming**: Focus on *what* you want, not *how* to implement it
- **Real-time Generation**: Watch mode automatically regenerates code on file changes
- **Human-in-the-Loop**: Preserves manual edits and optimizations
- **Local-First**: Everything runs on your machine with Ollama
- **Spanner Optimized**: Built-in best practices for Google Cloud Spanner

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

1. Create a declaration file (e.g., `user_queries.go`):

```go
package queries

import "time"

// GetUserRequest represents a request to fetch user information
// @description Retrieve user details by their unique ID from Spanner
type GetUserRequest struct {
    UserID string `json:"user_id"` // The unique identifier of the user
}

// GetUserResponse contains the user information
type GetUserResponse struct {
    ID        string    `json:"id"`
    Email     string    `json:"email"`
    Name      string    `json:"name"`
    CreatedAt time.Time `json:"created_at"`
    UpdatedAt time.Time `json:"updated_at"`
}
```

2. Generate implementation:

```bash
# One-time generation
glyph generate user_queries.go

# Or watch mode for continuous development
glyph watch user_queries.go
```

3. Glyph will generate `user_queries_impl.go` with the implementation:
   - Spanner-optimized SQL queries
   - Proper error handling
   - Context support
   - Best practices applied

4. Edit the declaration file and save - the implementation updates automatically!

## Configuration

Create `.glyph.yaml` in your home directory:

```yaml
model: devstral
host: http://localhost:11434
```

Or use environment variables:
- `OLLAMA_HOST`: Ollama server URL
- `GLYPH_MODEL`: AI model to use

## Commands

### Generate (One-time)
```bash
glyph generate <file> [flags]
```
Generates implementation once without watching. Perfect for CI/CD or integration with other tools.

### Watch (Interactive)
```bash
glyph watch <file> [flags]
```
Watches for file changes and regenerates automatically. Ideal for interactive development.

### Common Flags
```bash
  --config string   config file (default is $HOME/.glyph.yaml)
  --host string     Ollama host (default from OLLAMA_HOST env)
  --model string    AI model to use (default "devstral")
  -h, --help        help for glyph
```

## How It Works

1. **Declaration Parser**: Analyzes your Go structs using AST
2. **Context Builder**: Gathers declaration, existing code, and manual edits
3. **AI Generation**: Sends context to Ollama for implementation
4. **Code Generator**: Formats and writes the generated code
5. **File Watcher**: Monitors changes and triggers regeneration

## Development Workflow

1. Write declarative specifications in `*_queries.go` files
2. Let Glyph generate initial implementations
3. Manually optimize critical sections if needed
4. Your optimizations are preserved in future generations
5. Commit both declaration and implementation files

## License

MIT License - See LICENSE file for details