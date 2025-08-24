# Logging Guidelines

## Log Levels

### trace
- Detailed data flow and message contents
- Frequently occurring low-level operations
- Examples:
  - Full LSP message contents
  - CRDT operation details
  - Unsupported LSP features

### debug  
- Important processing flow
- Function entry/exit points
- Notable state changes
- Examples:
  - Target generation start
  - Document application
  - Format execution

### info
- User-facing progress
- Major processing milestones
- Configuration loading
- Examples:
  - Config file discovery
  - Generation completion

### warn
- Recoverable problems
- Fallback processing
- Examples:
  - Notification handler failures
  - LSP server shutdown issues

### error
- Fatal problems requiring user intervention
- Processing cannot continue

## Component-based Filtering

Use RUST_LOG environment variable for fine-grained control:

```bash
# All mantra debug logs
RUST_LOG=mantra=debug

# LSP trace logs only
RUST_LOG=mantra::lsp=trace

# Editor debug logs
RUST_LOG=mantra::editor=debug  

# Warnings + generation info
RUST_LOG=warn,mantra::generation=info

# Multiple components
RUST_LOG=mantra::lsp=trace,mantra::editor=debug
```

## Implementation Notes

- Target display is enabled in main.rs (`with_target(true)`)
- Default level: `warn,mantra=info`
- Use structured logging with tracing crate
- Avoid fmt::Printf debugging