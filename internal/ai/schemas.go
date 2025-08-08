package ai

import "encoding/json"

// ResponseFormat specifies the structured output format for AI responses
type ResponseFormat struct {
	Type       string          `json:"type"`        // "json_schema" for OpenRouter
	JSONSchema json.RawMessage `json:"json_schema"` // The actual JSON schema
}

// JSON Schema definitions for structured outputs
var (
	// ToolResponseSchema defines the base structure for tool responses
	ToolResponseSchema = json.RawMessage(`{
		"name": "tool_response",
		"strict": true,
		"schema": {
			"type": "object",
			"properties": {
				"tool": {"type": "string"},
				"result": {}
			},
			"required": ["tool", "result"]
		}
	}`)

	// SearchResultSchema defines the structure for search tool results
	SearchResultSchema = json.RawMessage(`{
		"name": "search_result",
		"strict": true,
		"schema": {
			"type": "object",
			"properties": {
				"found": {"type": "boolean"},
				"declarations": {
					"type": "array",
					"items": {
						"type": "object",
						"properties": {
							"name": {"type": "string"},
							"kind": {"type": "string"},
							"location": {"type": "string"}
						},
						"required": ["name", "kind"]
					}
				}
			},
			"required": ["found"]
		}
	}`)

	// InspectResultSchema defines the structure for inspect tool results
	InspectResultSchema = json.RawMessage(`{
		"name": "inspect_result",
		"strict": true,
		"schema": {
			"type": "object",
			"properties": {
				"found": {"type": "boolean"},
				"name": {"type": "string"},
				"kind": {"type": "string"},
				"package": {"type": "string"},
				"definition": {"type": "string"},
				"signature": {"type": "string"},
				"receiver": {"type": "string"},
				"implementation": {"type": "string"},
				"doc": {"type": "string"},
				"type": {"type": "string"},
				"value": {"type": "string"},
				"fields": {
					"type": "array",
					"items": {
						"type": "object",
						"properties": {
							"name": {"type": "string"},
							"type": {"type": "string"},
							"tag": {"type": "string"}
						},
						"required": ["name", "type"]
					}
				},
				"methods": {
					"type": "array",
					"items": {
						"type": "object",
						"properties": {
							"name": {"type": "string"},
							"signature": {"type": "string"},
							"receiver": {"type": "string"}
						},
						"required": ["name", "signature"]
					}
				}
			},
			"required": ["found", "name", "kind"]
		}
	}`)

	// ReadFuncResultSchema defines the structure for read_func tool results
	ReadFuncResultSchema = json.RawMessage(`{
		"name": "read_func_result",
		"strict": true,
		"schema": {
			"type": "object",
			"properties": {
				"found": {"type": "boolean"},
				"implementation": {"type": "string"},
				"error": {"type": "string"}
			},
			"required": ["found"]
		}
	}`)

	// CheckSyntaxResultSchema defines the structure for check_syntax tool results
	CheckSyntaxResultSchema = json.RawMessage(`{
		"name": "check_syntax_result",
		"strict": true,
		"schema": {
			"type": "object",
			"properties": {
				"valid": {"type": "boolean"},
				"errors": {
					"type": "array",
					"items": {"type": "string"}
				}
			},
			"required": ["valid"]
		}
	}`)

	// ContextGatheringSchema defines the structured output for context gathering phase
	ContextGatheringSchema = json.RawMessage(`{
		"name": "context_gathering_response",
		"strict": true,
		"schema": {
			"type": "object",
			"properties": {
				"success": {
					"type": "boolean",
					"description": "Whether context gathering succeeded"
				},
				"types": {
					"type": "array",
					"items": {
						"type": "object",
						"properties": {
							"name": {"type": "string"},
							"definition": {"type": "string"},
							"methods": {
								"type": "array",
								"items": {"type": "string"}
							}
						},
						"required": ["name", "definition"]
					}
				},
				"functions": {
					"type": "array",
					"items": {
						"type": "object",
						"properties": {
							"name": {"type": "string"},
							"signature": {"type": "string"},
							"implementation": {"type": "string"}
						},
						"required": ["name", "signature"]
					}
				},
				"constants": {
					"type": "array",
					"items": {
						"type": "object",
						"properties": {
							"name": {"type": "string"},
							"type": {"type": "string"},
							"value": {"type": "string"}
						},
						"required": ["name"]
					}
				},
				"error": {
					"type": "string",
					"description": "Error message if context gathering failed"
				}
			},
			"required": ["success"]
		}
	}`)

	// ImplementationSchema defines the structured output for implementation phase
	ImplementationSchema = json.RawMessage(`{
		"name": "implementation_response",
		"strict": true,
		"schema": {
			"type": "object",
			"properties": {
				"success": {
					"type": "boolean",
					"description": "Whether implementation succeeded"
				},
				"code": {
					"type": "string",
					"description": "The generated Go code implementation"
				},
				"error": {
					"type": "string",
					"description": "Error message if implementation failed"
				}
			},
			"required": ["success"]
		}
	}`)
)
