package ai

import "github.com/rail44/mantra/internal/tools"

// ConvertToAITools converts tools.Tool instances to ai.Tool format for API requests
func ConvertToAITools(toolList []tools.Tool) []Tool {
	aiTools := make([]Tool, len(toolList))
	for i, tool := range toolList {
		aiTools[i] = Tool{
			Type: "function",
			Function: ToolFunction{
				Name:        tool.Name(),
				Description: tool.Description(),
				Parameters:  tool.ParametersSchema(),
			},
		}
	}
	return aiTools
}
