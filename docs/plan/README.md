# Mantra Development Plans

This directory contains planning documents for the future development of Mantra.

## Documents

### [Rust Migration Plan](./rust-migration.md)
Detailed plan for migrating Mantra from Go to Rust, including:
- Phase breakdown and timeline
- Technical architecture
- Risk assessment
- Migration strategy

### [Feature Roadmap](./feature-roadmap.md)
Post-migration feature plans, including:
- Context enhancement capabilities
- Multi-language support via tree-sitter and LSP
- Learning and adaptation features
- Developer experience improvements

## Current Status

**Phase**: Planning
**Go Version**: Feature freeze (maintenance only)
**Rust Version**: Not started

## Priorities

1. **Immediate**: Complete Rust migration with feature parity
2. **Short-term**: Add project-specific context support
3. **Medium-term**: Implement multi-language support
4. **Long-term**: IDE integration and advanced features

## Decision Log

### Why Rust?
- Better performance for AST parsing
- Native tree-sitter support
- Memory safety without GC
- Single binary distribution
- Foundation for multi-language support

### Why Feature Freeze on Go?
- Avoid duplicate implementation effort
- Focus resources on single codebase
- Minimize migration complexity
- Maintain stability for existing users

## Contributing

These plans are living documents. Feedback and suggestions are welcome through GitHub issues.