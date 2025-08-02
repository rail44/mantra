# Complex Examples for Mantra

This directory contains complex, real-world examples to test Mantra's code generation capabilities with detailed Japanese instructions.

## Overview

These examples demonstrate:
- Complex business logic with multiple requirements
- Detailed Japanese instructions with technical specifications
- Integration with external services and dependencies
- Error handling and edge cases
- Performance considerations
- Security requirements

## Files

### 1. `repository.go` - User Management Service
Tests generation of:
- CRUD operations with caching
- Password hashing with bcrypt
- Transaction handling
- Email notifications
- Authentication with timing attack prevention
- Complex filtering and pagination

### 2. `analytics.go` - Analytics & Event Tracking Service
Tests generation of:
- High-throughput event processing
- Real-time aggregations
- Complex queries with multiple dimensions
- Cohort analysis
- Data export with streaming
- Rate limiting and batching

### 3. `payment.go` - Payment Processing Service
Tests generation of:
- Multi-provider payment processing
- Idempotency handling
- Risk assessment integration
- Subscription management
- Refund processing
- PCI compliance considerations
- Webhook handling

## Configuration

The `mantra.toml` file demonstrates:
- OpenRouter configuration with Cerebras provider
- Environment variable usage for API keys
- Debug logging for detailed output

## Usage

1. Set up your OpenRouter API key:
```bash
export OPENROUTER_API_KEY="your-api-key"
```

2. Run Mantra from this directory:
```bash
cd examples/complex
mantra generate .
```

3. Check the generated code in `./generated/` directory

## Testing Different Models

You can modify `mantra.toml` to test with different models:

```toml
# For Claude-3 Opus
model = "anthropic/claude-3-opus"

# For GPT-4
model = "openai/gpt-4"

# For Mixtral
model = "mistralai/mixtral-8x7b-instruct"
```

## Testing Different Providers

Modify the `[openrouter]` section to route through different providers:

```toml
[openrouter]
# For fastest generation
providers = ["Cerebras"]

# For highest quality
providers = ["Anthropic"]

# For cost optimization
providers = ["DeepInfra", "Together"]
```

## What to Look For

When reviewing generated code, check for:

1. **Correctness**: Does the implementation match all requirements?
2. **Completeness**: Are all edge cases handled?
3. **Error Handling**: Proper error messages and handling?
4. **Performance**: Efficient algorithms and data structures?
5. **Security**: Proper validation and security measures?
6. **Go Idioms**: Following Go best practices?
7. **Japanese Understanding**: Correct interpretation of Japanese instructions?

## Benchmarking

These examples are useful for:
- Comparing different AI models
- Testing generation speed
- Evaluating code quality
- Identifying model limitations
- Optimizing prompts

## Adding New Examples

When adding new test cases:
1. Include complex, multi-step requirements
2. Mix Japanese and English technical terms
3. Specify edge cases and error scenarios
4. Include performance requirements
5. Add security considerations
6. Reference external interfaces/types