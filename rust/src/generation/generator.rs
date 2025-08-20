use crate::parser::target::Target;
use actix::prelude::*;
use anyhow::Result;
use tracing::debug;

use crate::workspace::{GetLlmClient, Workspace};

/// Generate code for a specific target
pub async fn generate_for_target(
    target: &Target,
    workspace_addr: Addr<Workspace>,
) -> Result<String> {
    debug!("Got target info for {}", target.name);

    // Build prompt using generation module
    let prompt = super::build_prompt(target);

    // Get LLM client and generate code
    let llm_client = workspace_addr.send(GetLlmClient).await?;

    // Set provider if OpenRouter is configured
    let provider = llm_client
        .openrouter_config()
        .map(|config| crate::llm::ProviderSpec {
            only: Some(config.providers.clone()),
        });

    let request = crate::llm::CompletionRequest {
        model: llm_client.model().to_string(),
        provider,
        messages: vec![crate::llm::Message::user(prompt)],
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
