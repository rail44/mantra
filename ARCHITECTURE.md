# Mantra Architecture Documentation

## Overview
Mantra is a local-first AI-powered Go code generation tool that uses a two-phase approach to transform natural language instructions into working implementations.

## Core Architecture

### Two-Phase Generation System

```
┌─────────────────────────────────────────────────────────┐
│                     User Source Code                      │
│                  (with // mantra: comments)               │
└─────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│                    Target Detection                       │
│                 (internal/detector/)                      │
│         • Find functions with mantra comments             │
│         • Check checksums for changes                     │
└─────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│                  Parallel Execution                       │
│                  (internal/coder/)                        │
│         • Execute multiple targets concurrently           │
│         • Manage UI progress display                      │
└─────────────────────────────────────────────────────────┘
                            │
                   ┌────────┴────────┐
                   ▼                 ▼
┌──────────────────────┐  ┌──────────────────────┐
│   Phase 1: Context   │  │  Phase 2: Implement  │
│    (Temperature 0.6)  │  │   (Temperature 0.2)  │
│                      │  │                      │
│  Tools:              │  │  Tools:              │
│  • search            │  │  • check_code        │
│  • inspect           │  │  • result            │
│  • read_func         │  │                      │
│  • result            │  │  Input: Context from │
│                      │  │         Phase 1      │
└──────────────────────┘  └──────────────────────┘
                   │                 │
                   └────────┬────────┘
                            ▼
┌─────────────────────────────────────────────────────────┐
│                    Code Generation                        │
│                  (internal/codegen/)                      │
│         • AST manipulation                                │
│         • Import management                               │
│         • File writing to dest/                           │
└─────────────────────────────────────────────────────────┘
```

## Package Structure

### Entry Points
- `cmd/generate.go` (83 lines) - Thin CLI layer using Cobra
- `cmd/root.go` - Root command definition

### Application Layer
- `internal/app/generate.go` (313 lines) - Main orchestration logic
  - Target detection coordination
  - AI client setup
  - Result processing

### Core Generation Pipeline

#### Detection & Analysis
- `internal/detector/` - Find and analyze mantra targets
  - Checksum comparison
  - Status determination (new/outdated/current)
  
- `internal/parser/` - Parse Go source files
  - Extract targets and metadata
  - Handle function signatures and receiver methods

#### Code Generation
- `internal/coder/` - LLM-based code generation
  - `ParallelCoder`: Concurrent target execution
  - UI progress management
  
- `internal/codegen/` - Go source file generation
  - AST manipulation for function replacement
  - Import statement management
  - Checksum comment injection

#### LLM Integration
- `internal/llm/` - AI client implementation
  - `client.go`: Client initialization
  - `generation.go`: Main generation loop
  - `tool_executor.go`: Parallel tool execution
  - `openai.go`: OpenAI API implementation

#### Phase System
- `internal/phase/` - Two-phase execution
  - `interface.go`: Phase contract
  - `context_gathering.go`: Phase 1 implementation
  - `implementation.go`: Phase 2 implementation
  - `runner.go`: Phase execution coordinator

#### Tools
- `internal/tools/` - AI tool system
  - `interface.go`: Tool contract
  - `executor.go`: Tool execution management
  - `impl/search.go`: Code search tool
  - `impl/inspect.go`: Symbol inspection tool
  - `impl/check_code.go`: Staticcheck validation
  - `impl/result.go`: Result collection tool

#### Context Analysis
- `internal/context/` - Package and type analysis
  - `loader.go`: Package loading
  - `resolver.go`: Symbol resolution
  - `type_analyzer.go`: Type and method analysis
  - `context_extractor.go`: Context gathering for targets
  - `doc_extractor.go`: Documentation extraction

#### Support Systems
- `internal/ui/` - Terminal UI (Bubble Tea)
- `internal/log/` - Structured logging
- `internal/config/` - Configuration management
- `internal/checksum/` - Change detection
- `internal/imports/` - Import analysis
- `internal/formatter/` - Output formatting

## Data Flow

1. **Configuration Loading**
   - Read `mantra.toml` from project
   - Expand environment variables
   - Validate required fields

2. **Target Detection**
   - Scan Go files for `// mantra:` comments
   - Calculate checksums for change detection
   - Determine generation status

3. **Parallel Execution**
   - Create worker pool (max 16 concurrent)
   - Each target gets dedicated logger
   - UI shows real-time progress

4. **Phase 1: Context Gathering**
   - Temperature: 0.6 (exploratory)
   - Tools discover types, functions, imports
   - Output: Structured context JSON

5. **Phase 2: Implementation**
   - Temperature: 0.2 (deterministic)
   - Input: Context from Phase 1
   - Validation: Staticcheck analyzers
   - Output: Go code implementation

6. **Code Generation**
   - Parse existing file as AST
   - Replace function bodies
   - Add/update checksum comments
   - Manage imports
   - Write to destination directory

## Key Design Decisions

### Safety First
- Never modify original source files
- Generate to separate directory
- Preserve original panic("not implemented")
- Checksums prevent unnecessary regeneration

### Performance
- Parallel target execution
- Incremental generation via checksums
- Efficient AST manipulation
- Minimal file I/O

### Developer Experience
- Clear progress indication
- Structured logging levels
- Detailed error messages
- Support for multiple AI providers

### Extensibility
- Plugin-style tool system
- Phase-based architecture
- Clean interface boundaries
- Provider-agnostic LLM layer

## Configuration

### Required Settings
- `model`: AI model to use
- `url`: API endpoint
- `dest`: Output directory

### Optional Settings
- `api_key`: Authentication (supports env vars)
- `log_level`: error, warn, info, debug, trace
- `verbose`: Detailed logging flag
- `openrouter.providers`: Provider routing

## Error Handling

### Generation Failures
- Preserve original implementation
- Mark with `// mantra:failed:` comment
- Include failure reason and phase
- Continue with other targets

### Partial Success
- Each target independent
- Failed targets don't block others
- Clear failure reporting in logs

## Testing Strategy

- Unit tests for parser and tools
- Integration tests for phase system
- Example projects for end-to-end validation
- Manual testing with various AI providers