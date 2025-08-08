package phase

import (
	"context"
	"encoding/json"
	"fmt"
	"path/filepath"

	"github.com/rail44/mantra/internal/ai"
	"github.com/rail44/mantra/internal/formatter"
	"github.com/rail44/mantra/internal/log"
	"github.com/rail44/mantra/internal/parser"
	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/ui"
)

// Runner handles phase execution
type Runner struct {
	client *ai.Client
	logger log.Logger
}

// NewRunner creates a new phase runner
func NewRunner(client *ai.Client, logger log.Logger) *Runner {
	return &Runner{
		client: client,
		logger: logger,
	}
}

// ExecuteContextGathering executes the context gathering phase
func (r *Runner) ExecuteContextGathering(ctx context.Context, target *parser.Target, fileContent string, targetNum int, uiProgram *ui.Program) (map[string]interface{}, *parser.FailureReason) {
	r.logger.Info("Starting generation")
	uiProgram.UpdatePhase(targetNum, "Context Gathering", "Starting")

	// Setup phase
	r.logger.Info("Analyzing codebase context...")
	packagePath := filepath.Dir(target.FilePath)
	contextPhase := NewContextGatheringPhase(0.6, packagePath, r.logger)
	contextPhase.Reset() // Ensure clean state
	r.configureClientForPhase(contextPhase, nil)

	// Build prompt
	contextPromptBuilder := contextPhase.GetPromptBuilder()
	initialPrompt, err := contextPromptBuilder.BuildForTarget(target, fileContent)
	if err != nil {
		r.logger.Error("Failed to build prompt", "error", err.Error())
		return nil, &parser.FailureReason{
			Phase:   "context_gathering",
			Message: "Failed to build context gathering prompt: " + err.Error(),
			Context: "Prompt construction error",
		}
	}

	// Execute
	uiProgram.UpdatePhase(targetNum, "Context Gathering", "Analyzing codebase")
	_, err = r.client.Generate(ctx, initialPrompt)
	if err != nil {
		r.logger.Error("Context gathering failed", "error", err.Error())
		return nil, &parser.FailureReason{
			Phase:   "context_gathering",
			Message: "AI context gathering failed: " + err.Error(),
			Context: "May be due to insufficient codebase information or AI service issues",
		}
	}

	// Process result
	return r.processResult(contextPhase, "context_gathering")
}

// ExecuteImplementation executes the implementation phase
func (r *Runner) ExecuteImplementation(ctx context.Context, target *parser.Target, fileContent string, fileInfo *parser.FileInfo, projectRoot string, contextResult map[string]interface{}, targetNum int, uiProgram *ui.Program) (string, *parser.FailureReason) {
	r.logger.Info("Generating implementation...")
	uiProgram.UpdatePhase(targetNum, "Implementation", "Preparing")

	// Setup phase
	implPhase := NewImplementationPhase(0.2, projectRoot, r.logger)
	implPhase.Reset() // Ensure clean state

	// Create tool context for static analysis
	toolContext := tools.NewContext(fileInfo, target, projectRoot)
	r.configureClientForPhase(implPhase, toolContext)

	// Build prompt with context
	contextResultMarkdown := formatter.FormatContextAsMarkdown(contextResult)
	implPromptBuilder := implPhase.GetPromptBuilderWithContext(contextResultMarkdown)
	implPrompt, err := implPromptBuilder.BuildForTarget(target, fileContent)
	if err != nil {
		r.logger.Error("Failed to build implementation prompt", "error", err.Error())
		return "", &parser.FailureReason{
			Phase:   "implementation",
			Message: "Failed to build implementation prompt: " + err.Error(),
			Context: "Error occurred while incorporating context from phase 1",
		}
	}

	// Execute
	uiProgram.UpdatePhase(targetNum, "Implementation", "Generating code")
	_, err = r.client.Generate(ctx, implPrompt)
	if err != nil {
		r.logger.Error("Implementation failed", "error", err.Error())
		return "", &parser.FailureReason{
			Phase:   "implementation",
			Message: "AI implementation generation failed: " + err.Error(),
			Context: "May be due to complex requirements or AI service issues",
		}
	}

	// Process result
	result, failureReason := r.processResult(implPhase, "implementation")
	if failureReason != nil {
		return "", failureReason
	}

	// Extract implementation code
	if result != nil {
		if code, hasCode := result["code"].(string); hasCode {
			return code, nil
		}
		return "", &parser.FailureReason{
			Phase:   "implementation",
			Message: "Missing code field in successful result",
			Context: "The result() tool was called with success=true but no code was provided",
		}
	}

	return "", &parser.FailureReason{
		Phase:   "implementation",
		Message: "No result from implementation phase",
		Context: "Unexpected state",
	}
}

// processResult processes the result from a phase
func (r *Runner) processResult(p Phase, phaseName string) (map[string]interface{}, *parser.FailureReason) {
	phaseResult, completed := p.GetResult()
	if !completed {
		r.logger.Warn(fmt.Sprintf("%s phase did not complete with result tool", phaseName))
		return nil, &parser.FailureReason{
			Phase:   phaseName,
			Message: "Phase did not complete properly",
			Context: "The result() tool was not called",
		}
	}

	resultMap, ok := phaseResult.(map[string]interface{})
	if !ok {
		r.logger.Error(fmt.Sprintf("Unexpected result type from %s phase", phaseName), "type", fmt.Sprintf("%T", phaseResult))
		return nil, &parser.FailureReason{
			Phase:   phaseName,
			Message: fmt.Sprintf("Invalid result type from %s phase", phaseName),
			Context: fmt.Sprintf("Expected map, got %T", phaseResult),
		}
	}

	// Check for success/error structure
	if success, hasSuccess := resultMap["success"].(bool); hasSuccess {
		if !success {
			// Extract error information
			if errorField, hasError := resultMap["error"].(map[string]interface{}); hasError {
				message := ""
				details := ""
				if msg, ok := errorField["message"].(string); ok {
					message = msg
				}
				if det, ok := errorField["details"].(string); ok {
					details = det
				}
				return nil, &parser.FailureReason{
					Phase:   phaseName,
					Message: message,
					Context: details,
				}
			}
			return nil, &parser.FailureReason{
				Phase:   phaseName,
				Message: "Phase failed without error details",
				Context: "success=false but no error information",
			}
		}
		// Success - log and return
		if resultJSON, err := json.Marshal(resultMap); err == nil {
			r.logger.Debug(fmt.Sprintf("%s result", phaseName), "length", len(resultJSON))
			r.logger.Trace(fmt.Sprintf("%s output", phaseName), "content", string(resultJSON))
		}
		return resultMap, nil
	}

	return nil, &parser.FailureReason{
		Phase:   phaseName,
		Message: "Invalid result structure",
		Context: "The result() tool response is missing the success field",
	}
}

// configureClientForPhase configures the AI client with phase-specific settings
func (r *Runner) configureClientForPhase(p Phase, toolContext *tools.Context) {
	r.client.SetTemperature(p.GetTemperature())
	r.client.SetSystemPrompt(p.GetSystemPrompt())

	// Get tools once and convert/create executor
	phaseTools := p.GetTools()
	aiTools := ai.ConvertToAITools(phaseTools)
	executor := tools.NewExecutor(phaseTools, r.logger)

	// Set context if provided
	if toolContext != nil {
		executor.SetContext(toolContext)
	}

	r.client.SetTools(aiTools, executor)
}
