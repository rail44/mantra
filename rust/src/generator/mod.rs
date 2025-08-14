pub mod edit_event;

use anyhow::{Context, Result};
use std::path::Path;

use crate::config::Config;
use crate::llm::{CompletionRequest, LLMClient, Message, ProviderSpec};
use crate::lsp::{Client as LspClient, Position, TextDocumentIdentifier, TextDocumentItem};
use crate::parser::target::{get_function_type_positions, Target};

/// Code generator that handles the entire generation process
pub struct Generator {
    config: Config,
    client: LLMClient,
}

impl Generator {
    /// Create a new generator with the given configuration
    pub fn new(config: Config) -> Result<Self> {
        let client = LLMClient::new(config.clone())?;
        Ok(Self { config, client })
    }

    /// Generate code for a single file using incremental editing
    pub async fn generate_file(&self, file_path: &Path) -> Result<String> {
        use crate::generator::edit_event::EditEvent;
        use crate::incremental_editor::IncrementalEditor;

        // Create incremental editor
        let mut editor = IncrementalEditor::new(file_path)?;

        // Collect targets and their information before processing
        let targets_to_process: Vec<_> = {
            let target_map = editor.build_target_map()?;

            tracing::info!("Found {} targets in file", target_map.len());
            for target in target_map.targets() {
                tracing::info!("  - {} ({})", target.name, target.instruction);
            }

            // Collect all necessary information while target_map is valid
            target_map
                .checksums()
                .map(|checksum| {
                    let (target, node) = target_map.get(checksum).unwrap();
                    (
                        checksum,
                        target.clone(),
                        target_map.package_name().to_string(),
                        node.start_byte(),
                        node.end_byte(),
                    )
                })
                .collect()
        };

        // Process each target incrementally
        for (checksum, target, package_name, _start_byte, _end_byte) in targets_to_process {
            // Generate code and create EditEvent with node info
            let (event, node_start_byte) = {
                // Re-build target map for current state to get updated node
                let target_map = editor.build_target_map()?;

                // Find the node for this function by name
                let node_opt = target_map
                    .targets()
                    .zip(target_map.nodes())
                    .find(|(t, _)| t.name == target.name)
                    .map(|(_, n)| (n.clone(), n.start_byte()));

                if let Some((node, start_byte)) = node_opt {
                    // Generate code for this target
                    let generated_code = self
                        .generate_target_with_node(
                            &target,
                            &package_name,
                            file_path,
                            editor.source(),
                            &node,
                        )
                        .await?;

                    // Create EditEvent with checksum, signature, and generated code
                    (
                        Some(EditEvent::new(
                            checksum,
                            target.signature.clone(),
                            generated_code,
                        )),
                        start_byte,
                    )
                } else {
                    (None, 0)
                }
            };

            // Apply the EditEvent if we have one
            if let Some(event) = event {
                // Extract node info we need
                let node_info = {
                    let target_map = editor.build_target_map()?;
                    let mut result = None;
                    for node in target_map.nodes() {
                        if node.start_byte() == node_start_byte {
                            let func_start = node.start_byte();
                            if let Some(body_node) = node.child_by_field_name("body") {
                                result = Some((
                                    func_start,
                                    true,
                                    body_node.start_byte(),
                                    body_node.end_byte(),
                                ));
                            } else {
                                result = Some((func_start, false, 0, 0));
                            }
                            break;
                        }
                    }
                    result
                };

                let (func_start, has_body, body_start, body_end) = match node_info {
                    Some(info) => info,
                    None => continue,
                };

                editor.apply_edit_event(event, func_start, has_body, body_start, body_end)?;

                tracing::debug!(
                    "Applied edit for {} (checksum: {:x})",
                    target.name,
                    checksum
                );
            }
        }

        // Return the final edited source
        Ok(editor.source().to_string())
    }

    /// Generate code for a single target function with its node
    async fn generate_target_with_node(
        &self,
        target: &Target,
        package_name: &str,
        file_path: &Path,
        source: &str,
        node: &tree_sitter::Node<'_>,
    ) -> Result<String> {
        // Check for test mode
        if std::env::var("MANTRA_TEST_MODE").is_ok() {
            // Return mock response based on function name
            return Ok(match target.name.as_str() {
                "Add" => "return a + b",
                "IsEven" => "return n%2 == 0",
                "ToUpper" => "return strings.ToUpper(s)",
                "Multiply" => "return x * y",
                _ => "panic(\"not implemented\")",
            }
            .to_string());
        }

        // Collect type information from LSP
        let type_positions = get_function_type_positions(node);
        let type_info = match self
            .collect_type_info_at_positions(file_path, source, &type_positions)
            .await
        {
            Ok(info) => info,
            Err(e) => {
                tracing::warn!("Failed to get type info for {}: {}", target.name, e);
                "No type information available".to_string()
            }
        };

        // Build the prompt with type information
        let prompt = self.build_prompt_with_types(target, package_name, &type_info);

        tracing::debug!("Generating for target: {}", target.name);
        tracing::debug!("Prompt: {}", prompt);

        // Create the request
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

        // Send to LLM (OpenAI-compatible endpoint)
        let response = self.client.complete(request).await?;

        // Extract the generated code
        let generated = response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .context("No response from LLM")?;

        tracing::debug!("Raw LLM response: {}", generated);

        // Clean up the response (remove markdown if present)
        let cleaned = self.clean_generated_code(generated);
        tracing::debug!("Cleaned response: {}", cleaned);

        Ok(cleaned)
    }

    /// Collect type information at multiple positions using LSP
    async fn collect_type_info_at_positions(
        &self,
        file_path: &Path,
        source: &str,
        positions: &[(u32, u32)],
    ) -> Result<String> {
        if positions.is_empty() {
            return Ok("No type positions found".to_string());
        }

        // Start LSP client with notification support
        let lsp_client = LspClient::new("gopls", &[]).await?;

        // Initialize LSP
        // Convert to absolute path first
        let absolute_path = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            std::env::current_dir()?.join(file_path)
        };

        // Use parent directory as workspace root
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

        // Wait for diagnostics to ensure the file is analyzed
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

        let mut type_infos = Vec::new();

        // Get hover information for each position
        for (line, character) in positions.iter() {
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
                    // Extract the actual markdown content
                    let markdown_content = match hover.contents {
                        crate::lsp::MarkupContent::PlainText(text) => text,
                        crate::lsp::MarkupContent::Markdown { value, .. } => value,
                    };

                    tracing::info!("Type info at {}:{} - {}", line, character, markdown_content);
                    type_infos.push(markdown_content);
                }
                None => {
                    tracing::warn!("No hover information available at {}:{}", line, character);
                    // Don't include "No information" entries in the prompt
                }
            }
        }

        Ok(type_infos.join("\n\n"))
    }

    /// Build a prompt for the LLM with type information
    fn build_prompt_with_types(
        &self,
        target: &Target,
        package_name: &str,
        type_info: &str,
    ) -> String {
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

    /// Clean up generated code (remove markdown blocks, etc.)
    fn clean_generated_code(&self, code: String) -> String {
        let code = code.trim();

        // Remove markdown code blocks if present
        let code = if code.starts_with("```go") || code.starts_with("```") {
            let lines: Vec<&str> = code.lines().collect();
            let start = 1; // Skip the opening ```
            let end = lines.len().saturating_sub(1); // Skip the closing ```

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
