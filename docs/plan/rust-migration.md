# Rust Migration Plan

## Overview
Plan to migrate Mantra from Go to Rust and build a foundation for multi-language support.

## Migration Strategy

### Core Principles
- **Feature Freeze on Go Version**: No new features, focus on reimplementing existing functionality in Rust
- **Incremental Migration**: Port basic features first, ensuring stability at each step
- **Compatibility**: Maintain compatibility with existing mantra.toml configuration files

## Phase Breakdown

### Phase 1: Core Functionality (1-2 months)

#### 1.1 Foundation
- [ ] Project structure setup
- [ ] Configuration file loading (mantra.toml)
- [ ] LLM client implementation (OpenAI API compatible)
- [ ] Logging system

#### 1.2 File Processing
- [ ] Go file parsing (using tree-sitter-go)
- [ ] Mantra comment detection
- [ ] Checksum calculation and comparison
- [ ] Target detection (equivalent to detector)

#### 1.3 Code Generation Pipeline
- [ ] Two-phase system (Context Gathering / Implementation)
- [ ] Tool implementation (search, inspect, check_code, result)
- [ ] Prompt builder
- [ ] Generated code output

### Phase 2: Advanced Features (2-3 weeks)

#### 2.1 Context Analysis
- [ ] Type information extraction
- [ ] Package loader equivalent functionality
- [ ] Import analysis

#### 2.2 Parallel Processing and UI
- [ ] Parallel target execution
- [ ] TUI progress display (using ratatui)
- [ ] Improved error handling

#### 2.3 Validation
- [ ] Staticcheck equivalent validation (consider cargo-expand/clippy integration)
- [ ] Code modification via AST manipulation

### Phase 3: Feature Parity Verification (1 week)
- [ ] Output comparison tests with Go version
- [ ] Performance benchmarking
- [ ] Documentation updates
- [ ] Migration guide creation

## Technology Stack

### Core Dependencies
```toml
[dependencies]
tokio = "1.40"          # Async runtime
tree-sitter = "0.20"    # Syntax parsing
tree-sitter-go = "0.20" # Go language support
serde = "1.0"           # Serialization
toml = "0.8"            # Configuration files
reqwest = "0.11"        # HTTP client for LLM APIs
clap = "4.0"            # CLI parser
tracing = "0.1"         # Logging/tracing
anyhow = "1.0"          # Error handling
ratatui = "0.26"        # TUI
```

### Architecture Design

```rust
// Core structures and traits

pub struct Mantra {
    config: Config,
    llm_client: Box<dyn LLMClient>,
    parser: GoParser,  // tree-sitter-go based
}

pub trait Tool {
    fn name(&self) -> &str;
    fn execute(&self, params: Value) -> Result<Value>;
}

pub struct Phase {
    temperature: f32,
    tools: Vec<Box<dyn Tool>>,
    system_prompt: String,
}

pub struct Target {
    name: String,
    signature: String,
    instruction: String,
    checksum: String,
}
```

## Benefits After Migration

1. **Performance Improvements**
   - Faster AST parsing
   - Memory safety guarantees
   - True parallelism

2. **Multi-language Support Foundation**
   - Easy language addition with tree-sitter base
   - LSP integration groundwork

3. **Simplified Distribution**
   - Single binary distribution
   - Cross-compilation support

## Risks and Mitigations

### Risks
- tree-sitter-go may have more limited features than go/ast
- Rust learning curve (though developer is already proficient)
- Dual maintenance during migration period

### Mitigations
- Feature freeze on Go version minimizes maintenance burden
- Implement missing features incrementally
- Comprehensive test suite for quality assurance

## Timeline Estimate

- **Month 1-2**: Phase 1 complete (basic functionality)
- **Month 2-3**: Phase 2 complete (full feature port)
- **Month 3**: Phase 3 complete (release preparation)

## Next Steps

1. Set up Rust project structure
2. Validate tree-sitter-go capabilities
3. Begin minimal working implementation