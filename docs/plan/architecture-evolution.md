# mantra Architecture Evolution: Towards Pluggable Domain Knowledge

## Overview

This document outlines the evolution of mantra from its current generic Go code generation tool to a framework with pluggable domain knowledge modules.

## Current State (v1.0)

mantra is now a generic Go code generation tool that:
- Generates implementations for any Go function based on natural language instructions
- Works with any domain (not limited to Spanner or databases)
- Uses general-purpose prompts without domain-specific assumptions
- Supports dynamic code exploration through the tool system

## Target State

A flexible code generation tool that can adapt to various domains:
- Pluggable knowledge modules
- Configurable generation modes
- Community-contributed patterns

## Proposed Architecture

### Domain Knowledge System

```go
type DomainKnowledge interface {
    Name() string
    EnrichPrompt(prompt string, target *parser.Target) string
    GetDomainContext() string
    SuggestImports(target *parser.Target) []string
    GetBestPractices() map[string]string
    ValidateGenerated(code string, target *parser.Target) []string
}
```

### Configuration

```yaml
# .mantra.yaml
model: devstral
domains:  # Optional: Enable specific domain knowledge
  - spanner  # Cloud Spanner best practices
  - grpc     # gRPC service patterns
  - testing  # Test generation patterns

# Domain-specific settings (optional)
spanner:
  knowledge: ~/.mantra/knowledge/spanner.md
  patterns: ~/.mantra/patterns/spanner/
  
grpc:
  knowledge: ~/.mantra/knowledge/grpc.md
  proto_path: ./proto/
```

### Domain Management

```bash
# List available domain knowledge modules
mantra domain list

# Enable domain knowledge for current project
mantra domain enable spanner grpc

# Install community domain knowledge
mantra domain install github.com/user/mantra-redis-patterns

# Create custom domain knowledge
mantra domain create my-patterns --template basic
```

## Implementation Roadmap

### Phase 1: Domain Knowledge Interface (v1.1)
- Define domain knowledge plugin interface
- Create example domain modules
- Keep current generic behavior as default

### Phase 2: Domain System (v1.2)
- Implement domain loading and composition
- Add configuration support
- Create domain development documentation

### Phase 3: Core Domains (v1.3)
- Spanner patterns and best practices
- gRPC service patterns
- REST API patterns
- Testing patterns
- Error handling patterns

### Phase 4: Community Features (v2.0)
- Domain knowledge packaging
- Domain registry/marketplace
- Shared pattern libraries

## Domain Knowledge Examples

### Without Domain Knowledge (Current Default)
```go
// mantra: ユーザーをIDで取得
func GetUser(ctx context.Context, id string) (*User, error) {
    // Generic implementation based on context
}
```

### With Spanner Domain Knowledge
The same instruction would generate Spanner-optimized code with proper transaction handling.

### With gRPC Domain Knowledge
```go
// Input: service definition
type GetUserRequest struct {
    UserID string `json:"user_id"`
}

// Generated: gRPC handler
func (s *Server) GetUser(ctx context.Context, req *pb.GetUserRequest) (*pb.GetUserResponse, error) {
    // Implementation...
}
```

### REST Mode
```go
// Input: endpoint definition
type CreateUserRequest struct {
    Email string `json:"email" validate:"required,email"`
    Name  string `json:"name" validate:"required"`
}

// Generated: HTTP handler
func (h *Handler) CreateUser(w http.ResponseWriter, r *http.Request) {
    // Implementation...
}
```

## Benefits

1. **Broader adoption**: Appeals to more Go developers
2. **Community growth**: Users can contribute modes
3. **Knowledge sharing**: Best practices for each domain
4. **Flexibility**: Adapt to new technologies quickly

## Challenges

1. **API stability**: Mode interface must be well-designed
2. **Quality control**: Community modes vary in quality
3. **Documentation**: Each mode needs good docs
4. **Testing**: Ensure modes work correctly

## Success Criteria

- At least 5 well-maintained modes
- Active community contributions
- Positive user feedback on flexibility
- Maintained quality of generation

## Open Questions

1. Should domain knowledge be Go plugins or configuration-based?
2. How to compose multiple domain knowledge modules?
3. How to handle conflicting advice from different domains?
4. Should domain knowledge affect tool behavior?
5. How to ensure domain knowledge quality?

## References

- [Go Plugins](https://golang.org/pkg/plugin/)
- [Terraform Provider Architecture](https://www.terraform.io/docs/extend/how-terraform-works.html)
- [VS Code Extension API](https://code.visualstudio.com/api)