use anyhow::Result;

use crate::llm::{CompletionRequest, LLMClient, Message};
use crate::parser::target::Target;

/// Spawn a generation task that will send results back to the document service
pub async fn spawn_generation_task(target: &Target, llm_client: LLMClient) -> Result<String> {
    tracing::debug!(
        "Starting generation task for checksum {:x}",
        target.checksum
    );
    let new_body = generate_for_target(&llm_client, target).await?;
    tracing::debug!(
        "Completed generation task for checksum {:x}",
        target.checksum
    );
    Ok(new_body)
}

/// Generate code for a specific target using LLM
async fn generate_for_target(llm_client: &LLMClient, target: &Target) -> Result<String> {
    // Build prompt
    let prompt = super::build_prompt(target);

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
