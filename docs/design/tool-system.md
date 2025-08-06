# mantra Tool System Design

## Overview

The mantra Tool System enables LLMs to dynamically retrieve information during code generation, replacing the current static context extraction approach with an interactive, tool-based system.

## Tools

### 1. inspect

**Purpose**: Get detailed information about any declaration

**Parameters**:
```json
{
  "name": "string"  // Name of the declaration to inspect
}
```

**Returns**:
```json
{
  "found": "boolean",
  "name": "string",
  "kind": "string",  // "struct", "interface", "func", "method", "const", "var"
  "definition": "string",
  "fields": [...]     // For structs
  "methods": [...]    // For types with methods
  "signature": "..."  // For functions/methods
  "value": "..."      // For constants/variables
}
```

**Examples**:
```
inspect("UserRepository")
→ Interface definition with all method signatures

inspect("MaxRetries")
→ Constant definition with value
```

### 2. search

**Purpose**: Search for declarations using pattern matching

**Parameters**:
```json
{
  "pattern": "string",     // Search pattern (supports wildcards)
  "kind": "string",        // Optional: "struct", "interface", "method", "func", "const", "var"
  "limit": "integer"       // Optional: Maximum results (default: 10)
}
```

**Returns**:
```json
{
  "results": [
    {
      "name": "string",
      "kind": "string",
      "location": "string",
      "signature": "string"  // For functions/methods
    }
  ]
}
```

**Examples**:
```
search("*Repository", kind="struct")
→ Find all repository implementations

search("Create*", kind="method")
→ Find all Create methods
```

### 3. read_func

**Purpose**: Read the implementation of a function or method

**Parameters**:
```json
{
  "name": "string"  // Function/method name (e.g., "CreateUser" or "UserService.CreateUser")
}
```

**Returns**:
```json
{
  "found": "boolean",
  "name": "string",
  "signature": "string",
  "implementation": "string",  // The actual code
  "imports_used": ["..."],     // Imports referenced in the implementation
  "calls": ["..."]            // Functions/methods called
}
```

**Examples**:
```
read_func("UserService.CreateUser")
→ Full implementation with error handling patterns

read_func("PostgresUserRepository.GetByEmail")
→ Database query implementation example
```

### 4. check_syntax

**Purpose**: Validate Go syntax and optionally check for type safety

**Parameters**:
```json
{
  "code": "string",      // Go code to validate
  "context": "string",   // Optional: "function_body", "expression", or "statements"
  "type_check": "bool"   // Optional: Whether to perform type checking
}
```

**Returns**:
```json
{
  "valid": "boolean",
  "errors": [            // Array of syntax errors if any
    {
      "position": "string",
      "message": "string"
    }
  ],
  "suggestions": ["..."] // Optional: Fix suggestions
}
```

**Examples**:
```
check_syntax("return user, nil", context="function_body")
→ Valid syntax for function body

check_syntax("if user != nil {", context="statements")
→ Syntax error: missing closing brace
```

## Usage Flow

### Example: Implementing CreateUser method

```
Step 1: Understand the interface
→ inspect("UserRepository")
Result: Interface with GetByEmail, Create, Update, Delete methods

Step 2: Find implementations
→ search("*UserRepository", kind="struct")
Result: PostgresUserRepository, MockUserRepository

Step 3: Learn from existing implementations
→ read_func("PostgresUserRepository.Create")
Result: Implementation pattern with error handling

Step 4: Find similar methods
→ search("Create*", kind="method")
Result: CreateProduct, CreateOrder for pattern reference

Step 5: Read another example
→ read_func("ProductService.CreateProduct")
Result: Business logic pattern with validation

Step 6: Validate generated code syntax
→ check_syntax(generated_code, context="function_body")
Result: Valid Go syntax confirmed

Step 7: Generate CreateUser using learned patterns
```

## Implementation Architecture

### Directory Structure
```
internal/tools/
├── interface.go      # Tool interface definition
├── registry.go       # Tool registration and management
├── executor.go       # Tool execution engine
└── impl/
    ├── inspect.go      # inspect tool implementation
    ├── search.go       # search tool implementation
    ├── read_func.go    # read_func tool implementation
    └── check_syntax.go # check_syntax tool implementation
```

### Core Interfaces

```go
// Tool represents a tool that can be called by the AI
type Tool interface {
    Name() string
    Description() string
    ParametersSchema() json.RawMessage
    Execute(ctx context.Context, params map[string]interface{}) (interface{}, error)
}

// Registry manages available tools
type Registry interface {
    Register(tool Tool) error
    Get(name string) (Tool, error)
    ListAvailable() []ToolDefinition
}

// Executor handles tool execution with context
type Executor interface {
    Execute(ctx context.Context, toolName string, params map[string]interface{}) (interface{}, error)
}
```

## Benefits

1. **Dynamic Information Retrieval**: LLM requests only what it needs
2. **Reduced Context Size**: No need to include all possible information upfront
3. **Better Accuracy**: LLM can verify and correct as it generates
4. **Simpler Implementation**: Replace complex context extraction with focused tools
5. **Syntax Validation**: LLM can validate generated code before finalizing

## Migration Strategy

### Phase 1: Add Tools (Current context extraction remains)
- Implement the three tools
- Add tool support to AI providers
- Test with existing prompts

### Phase 2: Hybrid Approach
- Simplify context extraction
- Use tools for detailed information
- Measure improvement in generation quality

### Phase 3: Full Tool-based Generation
- Remove most static context extraction
- Rely primarily on tools
- Keep minimal context for efficiency

## Future Enhancements

1. **Type Relationship Cache**: Pre-compute interface implementations
2. **Advanced Filters**: More sophisticated search capabilities
3. **Cross-Project Analysis**: Learn from other Go projects
4. **Performance Optimization**: Caching and indexing strategies