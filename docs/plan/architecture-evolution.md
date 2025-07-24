# Mantra Architecture Evolution: From Specific to Generic

## Overview

This document outlines the planned evolution of Mantra from a Spanner-specific tool to a generic Go code generation framework with pluggable domain knowledge.

## Current State (v1.0)

Mantra currently focuses on generating Google Cloud Spanner data access layer code:
- Hardcoded Spanner best practices
- Spanner-specific prompt templates
- Fixed import statements for Spanner libraries

## Target State

A flexible code generation tool that can adapt to various domains:
- Pluggable knowledge modules
- Configurable generation modes
- Community-contributed patterns

## Proposed Architecture

### Mode System

```go
type GenerationMode interface {
    Name() string
    BuildPrompt(decl *Declaration) string
    GetSystemPrompt() string
    GetRequiredImports() []string
    GetTemplates() map[string]string
    ValidateOutput(code string) error
}
```

### Configuration

```yaml
# .mantra.yaml
mode: spanner  # or: grpc, rest, graphql, etc.
model: devstral

modes:
  spanner:
    knowledge: ~/.mantra/knowledge/spanner.md
    templates: ~/.mantra/templates/spanner/
    imports:
      - cloud.google.com/go/spanner
      - google.golang.org/api/iterator
  
  grpc:
    knowledge: ~/.mantra/knowledge/grpc.md
    templates: ~/.mantra/templates/grpc/
    imports:
      - google.golang.org/grpc
      - google.golang.org/protobuf
```

### Mode Management

```bash
# List available modes
mantra mode list

# Install a community mode
mantra mode install github.com/user/mantra-graphql-mode

# Create custom mode
mantra mode create my-mode --template spanner

# Set default mode
mantra mode set grpc
```

## Implementation Roadmap

### Phase 1: Extract Spanner Logic (v1.1)
- Move Spanner-specific code to a "spanner" mode
- Create mode interface
- Maintain backward compatibility

### Phase 2: Mode System (v1.2)
- Implement mode loading and switching
- Add configuration support
- Create mode development documentation

### Phase 3: Additional Modes (v1.3)
- gRPC service generation
- REST API handlers
- GraphQL resolvers
- Generic CRUD operations

### Phase 4: Community Features (v2.0)
- Mode packaging and distribution
- Mode marketplace/registry
- Shared knowledge bases

## Mode Examples

### Spanner Mode (Current)
Generates optimized Spanner queries with proper transaction handling.

### gRPC Mode
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

1. Should modes be Go plugins or configuration-based?
2. How to handle mode versioning?
3. What's the best distribution mechanism?
4. How to ensure mode quality?

## References

- [Go Plugins](https://golang.org/pkg/plugin/)
- [Terraform Provider Architecture](https://www.terraform.io/docs/extend/how-terraform-works.html)
- [VS Code Extension API](https://code.visualstudio.com/api)