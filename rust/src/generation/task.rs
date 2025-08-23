use anyhow::Result;
use tracing::{debug, error};

use crate::editor::crdt::CrdtEditor;
use crate::llm::{CompletionRequest, LLMClient, Message};
use crate::parser::target::Target;

/// Message to apply a generation result
pub struct ApplyGeneration {
    pub checksum: u64,
    pub new_body: String,
    pub snapshot: CrdtEditor,
}

/// Spawn a generation task that will send results back to the document service
pub async fn spawn_generation_task<F>(
    checksum: u64,
    target: Target,
    snapshot: CrdtEditor,
    llm_client: LLMClient,
    on_complete: F,
) where
    F: FnOnce(ApplyGeneration) -> futures::future::BoxFuture<'static, ()> + Send + 'static,
{
    tokio::spawn(async move {
        match generate_for_target(&llm_client, &target).await {
            Ok(new_body) => {
                let result = ApplyGeneration {
                    checksum,
                    new_body,
                    snapshot,
                };
                on_complete(result).await;
            }
            Err(e) => {
                error!("Failed to generate for {}: {}", target.name, e);
            }
        }
    });
}

/// Generate code for a specific target using LLM
async fn generate_for_target(llm_client: &LLMClient, target: &Target) -> Result<String> {
    debug!("Generating for target: {}", target.name);

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
