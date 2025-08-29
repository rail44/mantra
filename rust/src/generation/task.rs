use anyhow::Result;
use std::collections::HashMap;

use crate::inspector::SymbolInspector;
use crate::llm::{CompletionRequest, InspectTool, LLMClient, Message};
use crate::parser::target::Target;
use crate::workspace::WorkspaceService;

/// Spawn a generation task that will send results back to the document service
pub async fn spawn_generation_task(
    target: &Target,
    llm_client: LLMClient,
    workspace: &WorkspaceService,
) -> Result<String> {
    tracing::debug!(
        "Starting generation task for checksum {:x}",
        target.checksum
    );
    let new_body = generate_for_target(&llm_client, target, workspace).await?;
    tracing::debug!(
        "Completed generation task for checksum {:x}",
        target.checksum
    );
    Ok(new_body)
}

/// Generate code for a specific target using LLM
async fn generate_for_target(
    llm_client: &LLMClient,
    target: &Target,
    workspace: &WorkspaceService,
) -> Result<String> {
    // Collect detailed type definitions using SymbolInspector
    let inspector = SymbolInspector::new(workspace);
    let mut type_definitions = HashMap::new();
    let mut type_scope_mapping = HashMap::new();

    for type_ref in target.type_references.iter() {
        // Build mapping from scope_id to AST path for InspectTool
        type_scope_mapping.insert(type_ref.scope_id.clone(), type_ref.path.clone());

        match inspector.inspect_by_path(&target.uri, &type_ref.path).await {
            Ok(scoped_code) => {
                // Use scope_id as key instead of generic type_i
                tracing::debug!(
                    "Found detailed type definition for {}: {}",
                    type_ref.scope_id,
                    scoped_code.content
                );
                // Use the full type definition content instead of hover info
                type_definitions.insert(type_ref.scope_id.clone(), scoped_code.content);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to inspect type '{}' at path {:?}: {}",
                    type_ref.scope_id,
                    type_ref.path,
                    e
                );
                // Skip external types for now - they shouldn't be needed for basic generation
            }
        }
    }

    // Build prompts
    let system_prompt = super::build_system_prompt();
    let user_prompt = super::build_prompt_with_types(target, &type_definitions);
    tracing::debug!("System prompt:\n{}", system_prompt);
    tracing::debug!("User prompt:\n{}", user_prompt);

    // Generate using LLM with tool support
    let tools = vec![crate::llm::create_inspect_tool()];

    // Create InspectTool with necessary context
    let inspect_tool = InspectTool::new(workspace.clone(), target.uri.clone(), type_scope_mapping);

    // Handle conversation with tool calls
    let mut messages = vec![Message::system(system_prompt), Message::user(user_prompt)];
    let mut max_iterations = 5; // Prevent infinite loops

    loop {
        let current_request = CompletionRequest {
            model: llm_client.model().to_string(),
            provider: llm_client
                .openrouter_config()
                .map(|config| crate::llm::ProviderSpec {
                    only: Some(config.providers.clone()),
                }),
            messages: messages.clone(),
            max_tokens: Some(2000),
            temperature: 0.7,
            tools: Some(tools.clone()),
        };

        let response = llm_client.complete(current_request).await?;

        if let Some(choice) = response.choices.first() {
            let assistant_message = choice.message.clone();
            messages.push(assistant_message.clone());

            // Check if there are tool calls to execute
            if let Some(tool_calls) = &assistant_message.tool_calls {
                if max_iterations <= 0 {
                    tracing::warn!("Max tool call iterations reached");
                    break;
                }
                max_iterations -= 1;

                // Execute each tool call and add results
                for tool_call in tool_calls {
                    let result = match tool_call.function.name.as_str() {
                        "inspect" => inspect_tool.execute(tool_call).await,
                        _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_call.function.name)),
                    };

                    match result {
                        Ok(result) => {
                            tracing::debug!(
                                "Tool call executed: {} -> {}",
                                tool_call.function.name,
                                result.content
                            );
                            messages.push(Message::tool(result.content, result.tool_call_id));
                        }
                        Err(e) => {
                            tracing::error!("Tool call failed: {}", e);
                            messages
                                .push(Message::tool(format!("Error: {}", e), tool_call.id.clone()));
                        }
                    }
                }
                // Continue the loop to get the next response
            } else {
                // No tool calls, return the final response
                return Ok(super::clean_generated_code(assistant_message.content));
            }
        } else {
            return Err(anyhow::anyhow!("No response from LLM"));
        }
    }

    // Fallback if we exit the loop
    if let Some(last_message) = messages.last() {
        if last_message.role == crate::llm::types::Role::Assistant {
            return Ok(super::clean_generated_code(last_message.content.clone()));
        }
    }

    Err(anyhow::anyhow!("No valid response after tool calls"))
}
