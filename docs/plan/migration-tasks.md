# Rust Migration Task List

## Overview
This document outlines all tasks required for migrating Mantra from Go to Rust, organized by component and dependency order.

## Development Environment Setup

### Repository Structure
```
mantra/
├── rust/                 # Rust implementation (temporary during migration)
│   ├── Cargo.toml
│   ├── src/
│   └── tests/
├── internal/            # Go implementation (current)
├── examples/            # Shared test cases
└── docs/
```

### Branch Strategy
- Create `rust-migration` branch
- Develop in `rust/` directory
- After completion, replace entire repository with Rust version
- Archive Go version in `legacy-go` branch

## Minimal Viable Product (MVP) Definition

The MVP should be able to:
1. Read `mantra.toml` configuration
2. Parse Go files and detect `// mantra:` comments
3. Call LLM API with basic prompt
4. Generate code for a single function
5. Write output to destination directory

No requirements for MVP:
- No parallel processing
- No TUI
- No advanced context analysis
- No validation (staticcheck)
- Single-phase generation only

## Task Breakdown

### Phase 0: Project Setup
- [ ] Create `rust-migration` branch
- [ ] Initialize Rust project in `rust/` directory
- [ ] Setup `Cargo.toml` with initial dependencies
- [ ] Create basic directory structure
- [ ] Setup logging with `tracing`
- [ ] Create CLI skeleton with `clap`

### Phase 1: Core Components (MVP)

#### 1.1 Configuration (`src/config/`)
- [ ] Define `Config` struct matching Go version
- [ ] Implement TOML parsing
- [ ] Environment variable expansion
- [ ] Config file discovery (walk up directories)
- [ ] Validation of required fields

#### 1.2 Parser (`src/parser/`)
- [ ] Setup tree-sitter with tree-sitter-go
- [ ] Parse Go source files
- [ ] Extract function declarations
- [ ] Detect `// mantra:` comments
- [ ] Extract function signatures
- [ ] Calculate checksums (matching Go algorithm)
- [ ] Target detection (new vs existing)

#### 1.3 LLM Client (`src/llm/`)
- [ ] Define `LLMClient` trait
- [ ] OpenAI API client implementation
- [ ] Request/response structures
- [ ] Basic error handling
- [ ] API key management
- [ ] Temperature and model configuration

#### 1.4 Basic Generation (`src/generator/`)
- [ ] Simple prompt construction
- [ ] Single-phase generation (no context gathering)
- [ ] LLM response processing
- [ ] Code extraction from response

#### 1.5 File Output (`src/output/`)
- [ ] Create destination directory
- [ ] Write generated code to files
- [ ] Preserve file structure
- [ ] Basic import management

### Phase 2: Feature Parity

#### 2.1 Two-Phase System (`src/phase/`)
- [ ] Phase trait definition
- [ ] Context Gathering phase (temperature 0.6)
- [ ] Implementation phase (temperature 0.2)
- [ ] Phase runner
- [ ] Result passing between phases

#### 2.2 Tools (`src/tools/`)
- [ ] Tool trait definition
- [ ] Search tool (file/symbol search)
- [ ] Inspect tool (declaration lookup)
- [ ] Check code tool (validation)
- [ ] Result tool (phase completion)
- [ ] Tool executor

#### 2.3 Context Analysis (`src/analyzer/`)
- [ ] Type extraction from Go code
- [ ] Method detection
- [ ] Import analysis
- [ ] Package context
- [ ] Relevant context building

#### 2.4 Advanced Code Generation (`src/codegen/`)
- [ ] AST manipulation with tree-sitter
- [ ] Function body replacement
- [ ] Import statement management
- [ ] Checksum comment injection
- [ ] Multiple targets per file

#### 2.5 Parallel Execution (`src/executor/`)
- [ ] Tokio-based parallel processing
- [ ] Target scheduling
- [ ] Progress tracking
- [ ] Error aggregation

#### 2.6 UI (`src/ui/`)
- [ ] Ratatui TUI setup
- [ ] Progress bars for targets
- [ ] Real-time log display
- [ ] Error reporting in UI

### Phase 3: Optimization & Polish

#### 3.1 Performance
- [ ] File caching
- [ ] Parallel file parsing
- [ ] Connection pooling for LLM API
- [ ] Incremental parsing

#### 3.2 Error Handling
- [ ] Comprehensive error types
- [ ] Recovery mechanisms
- [ ] User-friendly error messages
- [ ] Debug mode with detailed logs

#### 3.3 Testing
- [ ] Unit tests for each module
- [ ] Integration tests
- [ ] Golden tests (output comparison with Go)
- [ ] Example projects testing

#### 3.4 Documentation
- [ ] Code documentation
- [ ] Migration guide
- [ ] API documentation
- [ ] Usage examples

## Implementation Order

### Week 1-2: MVP
1. Project setup
2. Configuration
3. Basic parser
4. Simple LLM client
5. Minimal generation
6. File output

**Milestone**: Can generate code for `examples/simple/simple.go`

### Week 3-4: Core Features
1. Two-phase system
2. Basic tools (search, result)
3. Better prompt construction
4. Checksum management

**Milestone**: Matching Go version for simple cases

### Week 5-6: Advanced Features
1. Full tool suite
2. Context analysis
3. Parallel execution
4. Error handling

**Milestone**: Feature parity with Go version

### Week 7-8: Polish
1. TUI
2. Performance optimization
3. Testing
4. Documentation

**Milestone**: Ready for replacement

## Technical Decisions

### Dependencies
```toml
[dependencies]
# Core
tokio = { version = "1", features = ["full"] }
anyhow = "1"
thiserror = "1"

# Parsing
tree-sitter = "0.20"
tree-sitter-go = "0.20"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# CLI
clap = { version = "4", features = ["derive"] }

# HTTP
reqwest = { version = "0.11", features = ["json"] }

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# UI (Phase 2)
ratatui = "0.26"
crossterm = "0.27"

# Utilities
regex = "1"
sha2 = "0.10"
walkdir = "2"
```

### Key Design Patterns

1. **Trait-based abstraction**
   - `LLMClient` trait for different providers
   - `Tool` trait for AI tools
   - `Phase` trait for generation phases
   - `Parser` trait for future language support

2. **Error handling**
   - `anyhow` for application errors
   - `thiserror` for library errors
   - Result<T> everywhere

3. **Async/await**
   - Tokio for runtime
   - Async traits where beneficial
   - Parallel processing with `tokio::spawn`

4. **Builder pattern**
   - For complex configurations
   - For prompt construction

## Success Criteria

### MVP Success
- [ ] Can read `examples/simple/simple.go`
- [ ] Detects all 6 mantra comments
- [ ] Generates some code (quality not critical)
- [ ] Writes to output directory

### Phase 2 Success
- [ ] Output matches Go version for simple examples
- [ ] All tools functioning
- [ ] Two-phase generation working

### Final Success
- [ ] Passes all Go version tests
- [ ] Performance within 2x of Go version
- [ ] No regressions in functionality
- [ ] Clean, maintainable code

## Open Questions

1. **Checksum algorithm**: Must match Go's exactly - need to verify
2. **Tree-sitter limitations**: Can it extract everything we need?
3. **LLM response parsing**: How to handle different response formats?
4. **Import management**: How closely to match Go's approach?

## MVP to Full Migration Path

### After MVP Completion

Once the MVP is working (can generate code for simple cases), the migration path becomes:

#### Step 1: Gradual Feature Addition
- Add remaining tools one by one
- Implement two-phase system
- Add context analysis incrementally
- Test each feature against Go version

#### Step 2: Replace Go Version
- [ ] Move `rust/` contents to repository root
- [ ] Archive Go code to `legacy/` directory
- [ ] Update CI/CD pipelines
- [ ] Update documentation
- [ ] Create migration guide for users

#### Step 3: Repository Restructure
```bash
# Current structure during migration
mantra/
├── rust/           # New Rust implementation
├── internal/       # Current Go implementation
└── examples/       # Shared test cases

# Final structure after migration
mantra/
├── src/            # Rust source (from rust/src/)
├── Cargo.toml      # Rust manifest (from rust/)
├── examples/       # Test cases (unchanged)
├── legacy/         # Archived Go code
└── docs/           # Updated documentation
```

#### Step 4: Release Strategy
1. **Alpha Release** (MVP)
   - Basic functionality only
   - Testing with early adopters
   - Collect feedback

2. **Beta Release** (Feature Complete)
   - All Go features ported
   - Performance testing
   - Bug fixes from alpha

3. **1.0 Release** (Stable)
   - Full documentation
   - Migration guide from Go version
   - Deprecation notice for Go version

### Migration Validation

#### Compatibility Testing
- [ ] All examples from Go version work
- [ ] Generated code is functionally equivalent
- [ ] Configuration files are compatible
- [ ] Command-line interface matches

#### Performance Benchmarks
- [ ] Startup time comparison
- [ ] Generation speed per target
- [ ] Memory usage
- [ ] Parallel processing efficiency

#### User Acceptance Criteria
- [ ] Existing users can migrate smoothly
- [ ] No regression in core functionality
- [ ] Improved or equal performance
- [ ] Better error messages

## Next Steps

1. ~~Create branch and setup Rust project~~ ✓
2. Implement configuration loading
3. Verify tree-sitter-go capabilities
4. Build minimal parser
5. Test with simple example