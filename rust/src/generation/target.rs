use anyhow::{Context, Result};
use std::path::Path;
use tree_sitter::Node;

use crate::config::Config;
use crate::llm::{CompletionRequest, LLMClient, Message, ProviderSpec};
use crate::lsp::{Client as LspClient, Position, TextDocumentIdentifier, TextDocumentItem};
use crate::parser::target::{get_function_type_positions, Target};

/// Generates code for a single target function
pub struct TargetGenerator {
    config: Config,
    client: LLMClient,
}

impl TargetGenerator {
    /// Create a new target generator
    pub fn new(config: Config) -> Result<Self> {
        let client = LLMClient::new(config.clone())?;
        Ok(Self { config, client })
    }

    /// Generate code for a single target with its AST node
    pub async fn generate(
        &self,
        target: &Target,
        package_name: &str,
        file_path: &Path,
        source: &str,
        node: &Node<'_>,
    ) -> Result<String> {
        // Check for test mode
        if std::env::var("MANTRA_TEST_MODE").is_ok() {
            return Ok(self.generate_test_response(target));
        }

        // Collect type information from LSP
        let type_positions = get_function_type_positions(node);
        let type_info = self
            .collect_type_info_at_positions(file_path, source, &type_positions)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to get type info for {}: {}", target.name, e);
                "No type information available".to_string()
            });

        // Build the prompt
        let prompt = self.build_prompt(target, package_name, &type_info);

        tracing::debug!("Generating for target: {}", target.name);
        tracing::debug!("Prompt: {}", prompt);

        // Send to LLM
        let response = self.send_to_llm(prompt).await?;

        // Clean and return the response
        Ok(self.clean_generated_code(response))
    }

    /// Generate test mode response
    fn generate_test_response(&self, target: &Target) -> String {
        match target.name.as_str() {
            "Add" => "return a + b",
            "IsEven" => "return n%2 == 0",
            "ToUpper" => "return strings.ToUpper(s)",
            "Multiply" => "return x * y",
            _ => "panic(\"not implemented\")",
        }
        .to_string()
    }

    /// Collect type information at specific positions using LSP
    async fn collect_type_info_at_positions(
        &self,
        file_path: &Path,
        source: &str,
        type_positions: &[(u32, u32)],
    ) -> Result<String> {
        if type_positions.is_empty() {
            return Ok("No type positions found".to_string());
        }

        // Start LSP client
        let lsp_client = LspClient::new("gopls", &[]).await?;

        // Initialize LSP
        let absolute_path = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            std::env::current_dir()?.join(file_path)
        };

        let workspace_root = absolute_path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".");

        let _init_result = lsp_client
            .initialize(
                Some(std::process::id()),
                Some(format!("file://{}", workspace_root)),
                serde_json::json!({
                    "textDocument": {
                        "hover": {
                            "contentFormat": ["markdown", "plaintext"]
                        },
                        "synchronization": {
                            "didOpen": true
                        }
                    }
                }),
                Some(vec![serde_json::json!({
                    "uri": format!("file://{}", workspace_root),
                    "name": "workspace"
                })]),
            )
            .await?;
        lsp_client.initialized().await?;

        // Open the document
        let file_uri = format!("file://{}", absolute_path.to_string_lossy());
        lsp_client
            .did_open(TextDocumentItem {
                uri: file_uri.clone(),
                language_id: "go".to_string(),
                version: 1,
                text: source.to_string(),
            })
            .await?;

        // Wait for diagnostics
        let timeout = std::time::Duration::from_secs(5);
        match lsp_client
            .wait_for_diagnostics_timeout(&file_uri, timeout)
            .await
        {
            Ok(diagnostics) => {
                tracing::debug!(
                    "Received {} diagnostics for {}",
                    diagnostics.diagnostics.len(),
                    file_uri
                );
            }
            Err(e) => {
                tracing::warn!("Failed to receive diagnostics: {}. Continuing anyway.", e);
            }
        }

        // Collect hover information
        let mut type_infos = Vec::new();
        for (line, character) in type_positions.iter() {
            match lsp_client
                .hover(
                    TextDocumentIdentifier {
                        uri: file_uri.clone(),
                    },
                    Position {
                        line: *line,
                        character: *character,
                    },
                )
                .await?
            {
                Some(hover) => {
                    let markdown_content = match hover.contents {
                        crate::lsp::MarkupContent::PlainText(text) => text,
                        crate::lsp::MarkupContent::Markdown { value, .. } => value,
                    };
                    tracing::info!("Type info at {}:{} - {}", line, character, markdown_content);
                    type_infos.push(markdown_content);
                }
                None => {
                    tracing::warn!("No hover information available at {}:{}", line, character);
                }
            }
        }

        Ok(type_infos.join("\n\n"))
    }

    /// Build prompt for LLM
    fn build_prompt(&self, target: &Target, package_name: &str, type_info: &str) -> String {
        let mut prompt = format!(
            "Generate the Go implementation for this function:\n\n\
             Package: {}\n\
             Function signature: {}\n\
             Instruction: {}",
            package_name, target.signature, target.instruction
        );

        if !type_info.is_empty() {
            prompt.push_str("\n\nType information:\n");
            prompt.push_str(type_info);
        }

        prompt.push_str("\n\nReturn only the code that goes inside the function body (without the curly braces).\n\
                        For example, if the function should add two numbers, just return: return a + b");

        prompt
    }

    /// Send request to LLM
    async fn send_to_llm(&self, prompt: String) -> Result<String> {
        let mut request = CompletionRequest {
            model: self.config.model.clone(),
            messages: vec![
                Message::system("You are a Go code generator. Generate only the function body implementation without the curly braces. Do not include the function signature. Do not include any comments or explanations. Do not use markdown code blocks. Return only the Go code that goes inside the function body."),
                Message::user(prompt),
            ],
            temperature: 0.2,
            max_tokens: Some(1000),
            provider: None,
        };

        // Add OpenRouter provider specification if configured
        if let Some(openrouter_config) = &self.config.openrouter {
            if !openrouter_config.providers.is_empty() {
                request.provider = Some(ProviderSpec {
                    only: Some(openrouter_config.providers.clone()),
                });
            }
        }

        let response = self.client.complete(request).await?;

        response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .context("No response from LLM")
    }

    /// Clean up generated code
    fn clean_generated_code(&self, code: String) -> String {
        let code = code.trim();

        // Remove markdown code blocks if present
        let code = if code.starts_with("```go") || code.starts_with("```") {
            let lines: Vec<&str> = code.lines().collect();
            let start = 1;
            let end = lines.len().saturating_sub(1);

            if end > start {
                lines[start..end].join("\n")
            } else {
                code.to_string()
            }
        } else {
            code.to_string()
        };

        // Remove curly braces if LLM included them
        let code = code.trim();
        if code.starts_with('{') && code.ends_with('}') {
            let inner = &code[1..code.len() - 1];
            return inner.trim().to_string();
        }

        code.to_string()
    }
}
