# Feature Roadmap

## Overview
This document outlines planned features and enhancements for Mantra after the Rust migration is complete.

## Core Philosophy
Mantra aims to be a domain-specific code generation tool that deeply understands project context and coding patterns, rather than a generic code completion tool.

## Feature Categories

### 1. Context Enhancement

#### 1.1 Code Update Support
**Problem**: When regenerating code, existing implementations are discarded entirely.

**Solution**: Include previous implementation as context when updating.

```toml
# Detection of changes
- Checksum change detected
- Load existing implementation
- Pass both original instruction and existing code to LLM
- "Update the implementation preserving good parts"
```

**Implementation**:
- Modify prompt builder to accept existing implementation
- Add update mode to generation pipeline
- Preserve manual modifications where possible

#### 1.2 Project-Specific Context
**Problem**: Each project has unique coding standards and patterns.

**Solution**: Allow projects to define custom context and rules.

```toml
# mantra.toml extension
[context]
system_prompt = """
Project-specific rules:
- Always use xerrors for error wrapping
- Follow repository pattern for database access
- Use structured logging (slog)
"""

context_files = [
    "docs/coding-standards.md",
    "docs/domain-model.md"
]

[templates.repository]
prompt = "Implement following repository pattern with transaction support"
examples = ["internal/repository/*.go"]
```

**Benefits**:
- Consistent code generation aligned with project standards
- Reduced need for manual corrections
- Domain-specific patterns built-in

### 2. Multi-Language Support

#### 2.1 Language Adapter Architecture
**Goal**: Support multiple programming languages through a unified interface.

```rust
trait LanguageAdapter {
    fn parse_file(&self, path: &Path) -> Result<SourceFile>;
    fn find_targets(&self, file: &SourceFile) -> Vec<Target>;
    fn generate_code(&self, target: &Target, implementation: &str) -> Result<String>;
}
```

#### 2.2 Supported Languages (Priority Order)
1. **Go** (current, via tree-sitter-go)
2. **Rust** (tree-sitter-rust + rust-analyzer)
3. **TypeScript** (tree-sitter-typescript + typescript-language-server)
4. **Python** (tree-sitter-python + pylsp)

#### 2.3 LSP Integration
**Purpose**: Accurate type information and project-wide understanding.

- Hover information for type details
- Go-to-definition for navigation
- Find-references for usage analysis
- Diagnostics for validation

### 3. Learning and Adaptation

#### 3.1 Pattern Recognition
**Goal**: Learn from successful generations and apply patterns.

```
.mantra/
  patterns/
    successful/     # Patterns that worked well
    failed/         # Patterns to avoid
    preferences/    # Coding style preferences
```

#### 3.2 Feedback Loop
```rust
// mantra: good implementation
func GetUser(id string) (*User, error) {
    // This implementation follows our patterns correctly
}

// mantra: bad implementation - reason: SQL injection vulnerability
func GetUserBad(id string) (*User, error) {
    // This has issues that should be avoided
}
```

### 4. Advanced Code Generation

#### 4.1 Multi-File Operations
**Current Limitation**: Can only generate within single files.

**Enhancement**: Support operations spanning multiple files.
- Generate interface and implementation in separate files
- Update related test files automatically
- Maintain consistency across module boundaries

#### 4.2 Refactoring Support
- Extract method/function
- Rename across project
- Move between packages
- Update all references

### 5. Developer Experience

#### 5.1 IDE Integration
**VS Code Extension** (minimal first version):
- Command palette integration
- Inline generation preview
- Diff view before applying

**LSP Server Mode**:
- Run Mantra as language server
- Real-time suggestions
- Code actions for generation

#### 5.2 Generation Transparency
```
$ mantra generate --explain

[Analysis Phase]
✓ Found type User with 5 fields
✓ Detected repository pattern in project
✓ Identified error handling style: fmt.Errorf

[Decision]
→ Using repository pattern template
→ Applying project-specific error wrapping
→ Including transaction support

[Generation]
→ Creating GetUserByEmail implementation...
```

## Implementation Priority

### After Rust Migration (Months 4-6)
1. **Project-specific context** - Immediate value for existing users
2. **Code update support** - Reduce regeneration friction
3. **Generation transparency** - Better debugging and trust

### Medium Term (Months 6-9)
1. **Rust language support** - Dogfooding on Mantra itself
2. **LSP integration** - Enhanced accuracy
3. **Pattern learning** - Adaptive generation

### Long Term (Months 9-12)
1. **TypeScript/Python support** - Broader user base
2. **Multi-file operations** - Complex refactoring
3. **IDE integration** - Seamless workflow

## Success Metrics

- **Generation Quality**: Reduction in manual fixes needed
- **Context Accuracy**: Fewer failed generations due to missing context
- **User Adoption**: Growth in active users and languages supported
- **Performance**: Generation time under 5 seconds for most targets

## Technical Debt to Address

- Improve error messages and recovery
- Add comprehensive test suite
- Document internal APIs
- Establish plugin architecture for extensions

## Open Questions

1. Should we support cloud-based LLM fine-tuning for project-specific models?
2. How to handle proprietary/closed-source language servers?
3. Should pattern learning be shared across projects (with privacy considerations)?
4. What's the right balance between automation and user control?

## Next Steps After Rust Migration

1. Implement project-specific context as first new feature
2. Gather user feedback on priorities
3. Create plugin API for community contributions
4. Establish regular release cycle