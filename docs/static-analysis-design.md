# Staticcheck Integration Design Document

## Overview
This document outlines the design and implementation strategy for integrating staticcheck as a library into Mantra's code generation workflow.

## Goals
- Provide comprehensive static analysis for AI-generated Go code
- Ensure type safety and catch common bugs early
- Integrate seamlessly with existing tool infrastructure
- Report issues with accurate position information relative to generated code

## Design Principles

### 1. Simplicity First
- No configuration levels or complex settings
- Default behavior provides all necessary static analysis
- Single responsibility: analyze generated code for issues

### 2. Library Integration
- Use staticcheck as a Go module, not as a subprocess
- Direct integration provides better performance and control
- Accept API instability risk (aligned with Mantra's WIP status)

### 3. AST-based Operations
- Use AST manipulation for reliable code transformation
- Leverage go/packages Overlay feature for in-memory analysis
- Maintain full type information from original context

## Implementation Strategy

### Core Components

#### StaticAnalysisTool Structure
```go
type StaticAnalysisTool struct {
    projectRoot string  // Required for type resolution
}
```

#### Modified File Tracking
```go
type ModifiedFile struct {
    Content      []byte         // Modified file content
    TargetFunc   *ast.FuncDecl  // Replaced function AST node
    BodyStartPos token.Pos      // New body start position
    BodyEndPos   token.Pos      // New body end position
    FileSet      *token.FileSet // For position resolution
}
```

#### Position Mapping
```go
type PositionMapper struct {
    funcDecl      *ast.FuncDecl
    bodyStart     token.Pos
    bodyEnd       token.Pos
    fileSet       *token.FileSet
    startPosition token.Position
}
```

### Processing Flow

1. **Receive Parameters**
   - Generated code (function body)
   - File information (parser.FileInfo)
   - Target function (parser.Target)

2. **AST Manipulation**
   - Parse original source file
   - Parse new function body
   - Replace function body in AST
   - Track position information

3. **Virtual File Creation**
   - Use go/packages Overlay feature
   - Create in-memory modified file
   - Maintain full context (imports, types)

4. **Static Analysis**
   - Load package with type information
   - Run all staticcheck analyzers
   - Filter diagnostics to generated code only

5. **Position Translation**
   - Convert absolute positions to relative
   - Report line numbers relative to function body
   - Adjust column positions for indentation

### Integration Points

#### Tool Interface
- Implements standard `tools.Tool` interface
- Compatible with existing tool executor
- Returns structured results in JSON format

#### Phase Integration
- Added to ImplementationPhase alongside check_syntax
- Provides deeper analysis after syntax validation
- Uses same context from ContextGatheringPhase

### Result Format

```json
{
  "valid": false,
  "issues": [
    {
      "code": "SA1000",
      "message": "Invalid regular expression",
      "line": 3,
      "column": 15
    }
  ]
}
```

Line and column numbers are relative to the generated function body, making it easy for the AI to identify and fix issues.

## Technical Decisions

### Why Use staticcheck as a Library?
- **Performance**: No process overhead, direct execution
- **Control**: Fine-grained control over analyzer execution
- **Integration**: Natural integration with Go toolchain
- **Completeness**: Access to all staticcheck analyzers

### Why AST Manipulation?
- **Reliability**: Guaranteed correct code transformation
- **Type Safety**: Preserves all type information
- **Position Tracking**: Accurate position mapping
- **Consistency**: Same approach as existing Generator

### Why Overlay?
- **In-memory**: No disk I/O required
- **Isolation**: Doesn't affect actual files
- **Type Resolution**: Full package context available
- **Standard API**: Uses official go/packages interface

## Implementation Checklist

- [ ] Create static_analysis.go with core implementation
- [ ] Add honnef.co/go/tools to go.mod
- [ ] Implement AST-based function replacement
- [ ] Create position mapper for relative positioning
- [ ] Integrate with ImplementationPhase
- [ ] Add comprehensive tests
- [ ] Update AI system prompt
- [ ] Document usage in README

## Future Enhancements

### Phase 1 (Current)
- Basic integration with all default analyzers
- Position-accurate error reporting
- Memory-efficient processing

### Phase 2 (Potential)
- Caching of analysis results
- Custom analyzer additions
- Performance optimizations
- Diagnostic severity levels

### Phase 3 (Potential)
- Auto-fix suggestions
- Incremental analysis
- Project-specific rules
- Integration with other linters

## Dependencies

```
honnef.co/go/tools v0.5.1  // Latest stable version
golang.org/x/tools         // For go/packages and analysis framework
```

## Notes

- API stability is not guaranteed by staticcheck, but this aligns with Mantra's WIP status
- All staticcheck analyzers are applied by default (no filtering)
- Position information is crucial for AI to understand and fix issues
- Integration maintains existing tool patterns for consistency