package generator

import (
	"fmt"
	"go/parser"
	"go/token"
	"strings"
)

// addImports adds required imports to the file content
func (g *Generator) addImports(content string, requiredImports []string) string {
	// Parse the content to find where to insert imports
	fset := token.NewFileSet()
	node, err := parser.ParseFile(fset, "", content, parser.ParseComments)
	if err != nil {
		// If parsing fails, try simple string manipulation
		return g.addImportsSimple(content, requiredImports)
	}

	// Check existing imports
	existingImports := make(map[string]bool)
	for _, imp := range node.Imports {
		path := strings.Trim(imp.Path.Value, `"`)
		existingImports[path] = true
	}

	// Filter out already existing imports
	var newImports []string
	for _, imp := range requiredImports {
		if !existingImports[imp] {
			newImports = append(newImports, imp)
		}
	}

	if len(newImports) == 0 {
		return content // No new imports needed
	}

	// Find the position to insert imports
	lines := strings.Split(content, "\n")
	packageLineIdx := -1
	existingImportStart := -1
	existingImportEnd := -1

	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "package ") {
			packageLineIdx = i
		} else if strings.HasPrefix(trimmed, "import (") {
			existingImportStart = i
			// Find the closing parenthesis
			for j := i + 1; j < len(lines); j++ {
				if strings.TrimSpace(lines[j]) == ")" {
					existingImportEnd = j
					break
				}
			}
			break
		} else if strings.HasPrefix(trimmed, "import ") {
			existingImportStart = i
			existingImportEnd = i
			break
		}
	}

	// Build the new content
	var result []string

	if existingImportStart >= 0 {
		// Add to existing imports
		result = append(result, lines[:existingImportEnd]...)
		for _, imp := range newImports {
			result = append(result, fmt.Sprintf("\t\"%s\"", imp))
		}
		result = append(result, lines[existingImportEnd:]...)
	} else if packageLineIdx >= 0 {
		// No existing imports, add new import block after package declaration
		result = append(result, lines[:packageLineIdx+1]...)
		result = append(result, "")
		result = append(result, "import (")
		for _, imp := range newImports {
			result = append(result, fmt.Sprintf("\t\"%s\"", imp))
		}
		result = append(result, ")")
		result = append(result, lines[packageLineIdx+1:]...)
	} else {
		// Fallback: just return original content
		return content
	}

	return strings.Join(result, "\n")
}

// convertBlankImports converts blank imports (_ "package") to regular imports
func (g *Generator) convertBlankImports(content string) string {
	lines := strings.Split(content, "\n")
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		// Check for blank import pattern: _ "package/path"
		if strings.HasPrefix(trimmed, `_ "`) {
			// Remove the underscore and space
			lines[i] = strings.Replace(line, `_ "`, `"`, 1)
		}
	}
	return strings.Join(lines, "\n")
}

// addImportsSimple adds imports using simple string manipulation
func (g *Generator) addImportsSimple(content string, requiredImports []string) string {
	// Find package declaration
	lines := strings.Split(content, "\n")
	packageLineIdx := -1

	for i, line := range lines {
		if strings.HasPrefix(strings.TrimSpace(line), "package ") {
			packageLineIdx = i
			break
		}
	}

	if packageLineIdx < 0 {
		return content // No package declaration found
	}

	// Build import block
	var importBlock []string
	importBlock = append(importBlock, "")
	importBlock = append(importBlock, "import (")
	for _, imp := range requiredImports {
		importBlock = append(importBlock, fmt.Sprintf("\t\"%s\"", imp))
	}
	importBlock = append(importBlock, ")")

	// Insert after package declaration
	result := make([]string, 0, len(lines)+len(importBlock))
	result = append(result, lines[:packageLineIdx+1]...)
	result = append(result, importBlock...)
	result = append(result, lines[packageLineIdx+1:]...)

	return strings.Join(result, "\n")
}
