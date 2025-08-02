package analysis

import (
	"go/ast"
	"strings"
)

// FieldInfo represents information about a struct field
type FieldInfo struct {
	Name string `json:"name"`
	Type string `json:"type"`
	Tag  string `json:"tag,omitempty"`
}

// FormatStructType formats a struct type in a readable way
func FormatStructType(s *ast.StructType) string {
	var result strings.Builder
	result.WriteString("struct {\n")

	if s.Fields != nil {
		for _, field := range s.Fields.List {
			result.WriteString("\t")

			// Field names
			if len(field.Names) > 0 {
				for i, name := range field.Names {
					if i > 0 {
						result.WriteString(", ")
					}
					result.WriteString(name.Name)
				}
				result.WriteString(" ")
			}

			// Field type
			result.WriteString(ExtractTypeString(field.Type))

			// Field tag
			if field.Tag != nil {
				result.WriteString(" ")
				result.WriteString(field.Tag.Value)
			}

			result.WriteString("\n")
		}
	}

	result.WriteString("}")
	return result.String()
}

// ExtractStructFields extracts field information from a struct type
func ExtractStructFields(s *ast.StructType) []FieldInfo {
	var fields []FieldInfo

	if s.Fields == nil {
		return fields
	}

	for _, field := range s.Fields.List {
		fieldType := ExtractTypeString(field.Type)

		if len(field.Names) == 0 {
			// Embedded field
			fields = append(fields, FieldInfo{
				Name: fieldType,
				Type: fieldType,
			})
		} else {
			// Named fields
			for _, name := range field.Names {
				fieldInfo := FieldInfo{
					Name: name.Name,
					Type: fieldType,
				}

				// Extract tag if present
				if field.Tag != nil {
					fieldInfo.Tag = field.Tag.Value
				}

				fields = append(fields, fieldInfo)
			}
		}
	}

	return fields
}
