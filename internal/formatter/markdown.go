package formatter

import (
	"fmt"
	"strings"
)

// FormatContextAsMarkdown converts the context gathering result from JSON to readable Markdown
func FormatContextAsMarkdown(contextResult map[string]interface{}) string {
	if contextResult == nil {
		return ""
	}

	var formatted strings.Builder

	// Format types section
	if types, ok := contextResult["types"].([]interface{}); ok && len(types) > 0 {
		formatted.WriteString("### Discovered Types\n\n")
		for _, t := range types {
			if typeMap, ok := t.(map[string]interface{}); ok {
				if name, ok := typeMap["name"].(string); ok {
					formatted.WriteString(fmt.Sprintf("#### %s\n", name))
				}
				if definition, ok := typeMap["definition"].(string); ok {
					formatted.WriteString(fmt.Sprintf("```go\n%s\n```\n", definition))
				}
				if methods, ok := typeMap["methods"].([]interface{}); ok && len(methods) > 0 {
					formatted.WriteString("**Methods:**\n")
					for _, method := range methods {
						if methodStr, ok := method.(string); ok {
							formatted.WriteString(fmt.Sprintf("- %s\n", methodStr))
						}
					}
				}
				formatted.WriteString("\n")
			}
		}
	}

	// Format functions section
	if functions, ok := contextResult["functions"].([]interface{}); ok && len(functions) > 0 {
		formatted.WriteString("### Discovered Functions\n\n")
		for _, f := range functions {
			if funcMap, ok := f.(map[string]interface{}); ok {
				if name, ok := funcMap["name"].(string); ok {
					formatted.WriteString(fmt.Sprintf("#### %s\n", name))
				}
				if signature, ok := funcMap["signature"].(string); ok {
					formatted.WriteString(fmt.Sprintf("```go\n%s\n```\n", signature))
				}
				if implementation, ok := funcMap["implementation"].(string); ok && implementation != "" {
					formatted.WriteString("**Implementation:**\n")
					formatted.WriteString(fmt.Sprintf("```go\n%s\n```\n", implementation))
				}
				formatted.WriteString("\n")
			}
		}
	}

	// Format constants section
	if constants, ok := contextResult["constants"].([]interface{}); ok && len(constants) > 0 {
		formatted.WriteString("### Discovered Constants/Variables\n\n")
		for _, c := range constants {
			if constMap, ok := c.(map[string]interface{}); ok {
				if name, ok := constMap["name"].(string); ok {
					formatted.WriteString(fmt.Sprintf("- **%s**", name))
					if typeStr, ok := constMap["type"].(string); ok && typeStr != "" {
						formatted.WriteString(fmt.Sprintf(" (`%s`)", typeStr))
					}
					if value, ok := constMap["value"].(string); ok && value != "" {
						formatted.WriteString(fmt.Sprintf(" = `%s`", value))
					}
					formatted.WriteString("\n")
				}
			}
		}
		formatted.WriteString("\n")
	}

	return formatted.String()
}
