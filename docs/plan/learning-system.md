# Mantra Learning System Design

## Overview

This document outlines the future design for Mantra's learning system, which will enable the tool to improve its code generation quality based on user behavior without requiring explicit training actions.

## Goals

1. **Zero-friction learning**: The system should learn from normal usage patterns
2. **Privacy-first**: All learning data remains local
3. **Progressive enhancement**: Quality improves over time automatically
4. **Opt-in complexity**: Advanced features available but not required

## Architecture

### 1. Implicit Learning from Daily Usage

The system will automatically collect learning signals during normal operation:

```go
// Automatic detection after generation
func (g *Generator) collectLearningData(generated string) {
    // Check for human edits after 5 seconds
    time.Sleep(5 * time.Second)
    
    current := readFile(g.outputPath)
    if current != generated {
        // Human edited = valuable learning data
        example := TrainingExample{
            Original: generated,
            Edited:   current,
            Quality:  "human-improved",
        }
        g.learningStore.SaveQuietly(example)
    }
}
```

### 2. Quality Signal Detection

The system will infer quality from various implicit signals:

- **Human edits**: Changes made after generation
- **Edit distance**: Amount of modification (less = better)
- **Time to edit**: Delay before editing (longer = better)
- **Compilation success**: Whether the code compiles
- **Test passage**: Whether tests pass (if detectable)
- **Git commits**: Whether code was committed

### 3. Automatic Model Improvement

Based on collected data, the system will periodically:

1. Update few-shot examples with high-quality samples
2. Optimize Modelfile parameters based on user preferences
3. Generate custom Ollama ADAPTER files (when sufficient data exists)

### 4. Ollama ADAPTER Integration

Leveraging Ollama's ADAPTER feature for lightweight model customization:

```
FROM devstral
ADAPTER ~/.mantra/adapters/user-style.gguf

TEMPLATE """{{ .System }}
{{ if .Examples }}
Reference examples:
{{ .Examples }}
{{ end }}

Task: {{ .Prompt }}
"""

SYSTEM """Customized based on user's coding patterns..."""
```

## User Experience

### Normal Usage (No Learning Awareness)

```bash
# User just uses Mantra normally
mantra generate user_queries.go
mantra watch api_handlers.go

# Learning happens automatically in background
```

### Initial Setup (Optional)

```bash
# Learn from existing project (one-time)
mantra init --learn-from ./my-project

# Output:
# Analyzing patterns...
# ✓ Found consistent use of ReadOnlyTransaction
# ✓ Detected error handling style
# ✓ Identified naming conventions
```

### Minimal Feedback (Optional)

```bash
# Only when user wants to provide explicit feedback
mantra feedback  # Rate last generation

# Or during generation
mantra generate user.go --feedback
```

## Implementation Phases

### Phase 1: Passive Data Collection
- File change detection
- Quality signal gathering
- Local storage of examples

### Phase 2: Few-shot Learning
- Automatic example selection
- Dynamic prompt enhancement
- Pattern recognition

### Phase 3: Model Customization
- Modelfile generation
- Parameter optimization
- ADAPTER file creation

### Phase 4: Advanced Features
- Cross-project learning
- Team pattern sharing
- Fine-tuning pipeline

## Privacy Considerations

- All data stored locally in `~/.mantra/`
- No network transmission of code
- Configurable anonymization for shared patterns
- Explicit opt-in for any sharing features

## Configuration

```yaml
# ~/.mantra/config.yaml
learning:
  mode: passive          # passive or active
  auto_improve: true     # Automatic model updates
  threshold: 50          # Examples needed before improvement
  privacy:
    local_only: true     # Never transmit data
    anonymize: true      # Strip identifiers
    exclude_patterns:    # Patterns to ignore
      - "*secret*"
      - "*private*"
```

## Future Considerations

1. **Integration with external fine-tuning tools** (Unsloth, etc.)
2. **Community pattern sharing** (with privacy controls)
3. **Multi-language support** beyond Go
4. **IDE integrations** for better signal collection

## Success Metrics

- Reduction in human edits over time
- Increased compilation success rate
- User satisfaction (optional surveys)
- Time saved in development

## References

- [Ollama Modelfile Documentation](https://github.com/ollama/ollama/blob/main/docs/modelfile.md)
- [LoRA: Low-Rank Adaptation of Large Language Models](https://arxiv.org/abs/2106.09685)
- [Few-shot Learning in LLMs](https://arxiv.org/abs/2005.14165)