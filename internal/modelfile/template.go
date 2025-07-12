package modelfile

import (
	"bytes"
	"fmt"
	"text/template"
)

// TemplateData represents data for rendering a Modelfile
type TemplateData struct {
	BaseModel    string
	SystemPrompt string
	Examples     []Example
	Parameters   map[string]interface{}
	Prompt       string
}

// Example represents a code example for few-shot learning
type Example struct {
	Input  string
	Output string
}

// DefaultTemplate is the standard Modelfile template
const DefaultTemplate = `FROM {{.BaseModel}}

SYSTEM """{{.SystemPrompt}}
{{if .Examples}}
Reference examples for this task:
{{range .Examples}}
Input:
{{.Input}}

Output:
{{.Output}}

{{end}}{{end}}
Generate only the function implementation without package declaration, imports, or type definitions.
Follow the patterns shown in the examples.
"""

{{range $key, $value := .Parameters}}
PARAMETER {{$key}} {{$value}}
{{end}}

TEMPLATE """{{ "{{" }} .System {{ "}}" }}

{{ "{{" }} .Prompt {{ "}}" }}"""
`

// RenderModelfile generates a Modelfile from template and data
func RenderModelfile(data TemplateData) (string, error) {
	tmpl, err := template.New("modelfile").Parse(DefaultTemplate)
	if err != nil {
		return "", fmt.Errorf("failed to parse template: %w", err)
	}

	var buf bytes.Buffer
	if err := tmpl.Execute(&buf, data); err != nil {
		return "", fmt.Errorf("failed to execute template: %w", err)
	}

	return buf.String(), nil
}