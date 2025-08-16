use anyhow::{Context, Result};
use std::collections::HashSet;
use tree_sitter::Node;

use crate::llm::{CompletionRequest, Message, ProviderSpec};
use crate::parser::target::Target;

/// Generates code for a single target function
pub struct TargetGenerator<'a> {
    target: &'a Target,
    package_name: &'a str,
    node: Node<'a>,
    file_uri: String,
}

impl<'a> TargetGenerator<'a> {
    /// Create a new target generator for a specific target
    pub fn new(
        target: &'a Target,
        package_name: &'a str,
        node: Node<'a>,
        file_uri: String,
    ) -> Self {
        Self {
            target,
            package_name,
            node,
            file_uri,
        }
    }

    /// Generate code for this target
    pub async fn generate(
        &self,
        workspace: tokio::sync::mpsc::Sender<crate::workspace::WorkspaceCommand>,
    ) -> Result<String> {
        // Collect type information using InspectTool
        let type_info = self
            .collect_type_info_with_inspect(workspace.clone())
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to get type info for {}: {}", self.target.name, e);
                "No type information available".to_string()
            });

        // Build the prompt
        let prompt = self.build_prompt(&type_info);

        tracing::debug!("Generating for target: {}", self.target.name);
        tracing::debug!("Prompt: {}", prompt);

        // Send to LLM
        let response = self.send_to_llm(workspace, prompt).await?;

        // Clean and return the response
        Ok(self.clean_generated_code(response))
    }

    /// Collect type information using InspectTool
    async fn collect_type_info_with_inspect(
        &self,
        workspace: tokio::sync::mpsc::Sender<crate::workspace::WorkspaceCommand>,
    ) -> Result<String> {
        // Extract type names from the function signature
        let type_names = self.extract_type_names_from_signature();

        if type_names.is_empty() {
            return Ok("No custom types found in signature".to_string());
        }

        // Create initial scope for the function
        let start_line = self.node.start_position().row as u32;
        let end_line = self.node.end_position().row as u32;

        // Register scope via Workspace
        let (tx, rx) = tokio::sync::oneshot::channel();
        workspace
            .send(crate::workspace::WorkspaceCommand::RegisterScope {
                uri: self.file_uri.clone(),
                range: crate::lsp::Range {
                    start: crate::lsp::Position {
                        line: start_line,
                        character: 0,
                    },
                    end: crate::lsp::Position {
                        line: end_line,
                        character: 0,
                    },
                },
                response: tx,
            })
            .await?;
        let scope_id = rx.await?;

        // Collect type definitions using InspectTool
        let mut type_definitions = Vec::new();
        let mut visited_types = HashSet::new();

        for type_name in type_names {
            if visited_types.contains(&type_name) {
                continue;
            }
            visited_types.insert(type_name.clone());

            tracing::debug!("Inspecting type: {}", type_name);

            // Try to inspect the type definition
            let inspect_request = crate::tools::inspect::InspectRequest {
                scope_id: scope_id.clone(),
                symbol: type_name.clone(),
            };

            // Send inspect request via Workspace
            let (tx, rx) = tokio::sync::oneshot::channel();
            match workspace
                .send(crate::workspace::WorkspaceCommand::InspectSymbol {
                    request: inspect_request,
                    response: tx,
                })
                .await
            {
                Ok(_) => {
                    // Wait for the response
                    match rx.await? {
                        Ok(response) => {
                            tracing::info!(
                                "Found definition for type {}: {}",
                                type_name,
                                response.code
                            );
                            type_definitions
                                .push(format!("// Definition of {}\n{}", type_name, response.code));
                        }
                        Err(e) => {
                            tracing::debug!("Could not find definition for {}: {}", type_name, e);
                            // Skip hover fallback for now
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("Could not send inspect command for {}: {}", type_name, e);
                    // Skip this type
                }
            }
        }

        if type_definitions.is_empty() {
            Ok("No type definitions found".to_string())
        } else {
            Ok(type_definitions.join("\n\n"))
        }
    }

    /// Extract type names from the function signature
    fn extract_type_names_from_signature(&self) -> Vec<String> {
        let mut type_names = Vec::new();
        let signature = &self.target.signature;

        // Simple regex-like extraction of type names
        // This is a simplified version - in production you'd want proper parsing
        let words: Vec<&str> = signature
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .collect();

        for word in words {
            if word.is_empty() {
                continue;
            }

            // Skip Go built-in types and keywords
            let is_builtin = matches!(
                word,
                "func"
                    | "struct"
                    | "interface"
                    | "map"
                    | "chan"
                    | "slice"
                    | "string"
                    | "int"
                    | "int8"
                    | "int16"
                    | "int32"
                    | "int64"
                    | "uint"
                    | "uint8"
                    | "uint16"
                    | "uint32"
                    | "uint64"
                    | "float32"
                    | "float64"
                    | "bool"
                    | "byte"
                    | "rune"
                    | "error"
                    | "any"
                    | "comparable"
                    | "context"
                    | "Context"
            );

            if !is_builtin && word.chars().next().is_some_and(|c| c.is_uppercase()) {
                type_names.push(word.to_string());
            }
        }

        // Remove duplicates while preserving order
        let mut seen = HashSet::new();
        type_names.retain(|name| seen.insert(name.clone()));

        type_names
    }

    /// Build prompt for LLM
    fn build_prompt(&self, type_info: &str) -> String {
        let mut prompt = format!(
            "Generate the Go implementation for this function:\n\n\
             Package: {}\n\
             Function signature: {}\n\
             Instruction: {}",
            self.package_name, self.target.signature, self.target.instruction
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
    async fn send_to_llm(
        &self,
        workspace: tokio::sync::mpsc::Sender<crate::workspace::WorkspaceCommand>,
        prompt: String,
    ) -> Result<String> {
        // Get LLM client via Workspace
        let (tx, rx) = tokio::sync::oneshot::channel();
        workspace
            .send(crate::workspace::WorkspaceCommand::GetLlmClient { response: tx })
            .await?;
        let llm_client = rx.await?;

        let mut request = CompletionRequest {
            model: llm_client.model().to_string(),
            messages: vec![
                Message::system("You are a Go code generator. Generate only the function body implementation without the curly braces. Do not include the function signature. Do not include any comments or explanations. Do not use markdown code blocks. Return only the Go code that goes inside the function body."),
                Message::user(prompt),
            ],
            temperature: 0.2,
            max_tokens: Some(1000),
            provider: None,
        };

        // Add OpenRouter provider specification if configured
        if let Some(openrouter_config) = llm_client.openrouter_config() {
            if !openrouter_config.providers.is_empty() {
                request.provider = Some(ProviderSpec {
                    only: Some(openrouter_config.providers.clone()),
                });
            }
        }

        let response = llm_client.complete(request).await?;

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
