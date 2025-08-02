package impl

import (
	"strings"
)

// matchesPattern checks if a name matches a wildcard pattern
func matchesPattern(name, pattern string) bool {
	// Handle wildcards
	if pattern == "*" {
		return true
	}

	// Convert pattern to simple regex-like matching
	if strings.HasPrefix(pattern, "*") && strings.HasSuffix(pattern, "*") {
		// *abc* - contains
		substr := pattern[1 : len(pattern)-1]
		return strings.Contains(name, substr)
	} else if strings.HasPrefix(pattern, "*") {
		// *abc - ends with
		suffix := pattern[1:]
		return strings.HasSuffix(name, suffix)
	} else if strings.HasSuffix(pattern, "*") {
		// abc* - starts with
		prefix := pattern[:len(pattern)-1]
		return strings.HasPrefix(name, prefix)
	}

	// Exact match
	return name == pattern
}
