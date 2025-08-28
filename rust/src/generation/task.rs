use anyhow::Result;
use std::collections::HashMap;

use crate::document::DocumentService;
use crate::inspector::SymbolInspector;
use crate::llm::{CompletionRequest, LLMClient, Message};
use crate::parser::target::Target;
use crate::workspace::WorkspaceService;

/// Spawn a generation task that will send results back to the document service
pub async fn spawn_generation_task(
    target: &Target,
    llm_client: LLMClient,
    document_service: DocumentService,
    workspace: &WorkspaceService,
) -> Result<String> {
    tracing::debug!(
        "Starting generation task for checksum {:x}",
        target.checksum
    );
    let new_body = generate_for_target(&llm_client, target, &document_service, workspace).await?;
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
    document_service: &DocumentService,
    workspace: &WorkspaceService,
) -> Result<String> {
    // Collect detailed type definitions using SymbolInspector
    let inspector = SymbolInspector::new(workspace);
    let mut type_definitions = HashMap::new();

    for (i, type_path) in target.type_references.iter().enumerate() {
        match inspector.inspect_by_path(&target.uri, type_path).await {
            Ok(scoped_code) => {
                let key = format!("type_{}", i);
                tracing::debug!("Found detailed type definition: {}", scoped_code.content);
                // Use the full type definition content instead of hover info
                type_definitions.insert(key, scoped_code.content);
            }
            Err(e) => {
                tracing::warn!("Failed to inspect type at path {:?}: {}", type_path, e);
                // Skip external types for now - they shouldn't be needed for basic generation
            }
        }
    }

    // Build prompt with type information
    let prompt = super::build_prompt_with_types(target, &type_definitions);
    tracing::debug!("Generated prompt:\n{}", prompt);

    // Generate using LLM
    let request = CompletionRequest {
        model: llm_client.model().to_string(),
        provider: llm_client
            .openrouter_config()
            .map(|config| crate::llm::ProviderSpec {
                only: Some(config.providers.clone()),
            }),
        messages: vec![Message::user(prompt)],
        max_tokens: Some(2000),
        temperature: 0.7,
    };

    let response = llm_client.complete(request).await?;

    if let Some(choice) = response.choices.first() {
        Ok(super::clean_generated_code(choice.message.content.clone()))
    } else {
        Err(anyhow::anyhow!("No response from LLM"))
    }
}
