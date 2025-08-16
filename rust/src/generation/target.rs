use anyhow::{Context, Result};
use tree_sitter::Node;

use crate::llm::{CompletionRequest, Message, ProviderSpec};
use crate::lsp::{Position, TextDocumentIdentifier};
use crate::parser::target::{get_function_type_positions, Target};
use crate::workspace::Workspace;

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
    pub async fn generate(&self, workspace: &mut Workspace) -> Result<String> {
        // Collect type information using InspectTool
        let type_info = self
            .collect_type_info_with_inspect(workspace)
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
    async fn collect_type_info_with_inspect(&self, workspace: &mut Workspace) -> Result<String> {
        // Extract type positions from the node
        let type_positions = get_function_type_positions(&self.node);
        
        if type_positions.is_empty() {
            return Ok("No type positions found".to_string());
        }

        // Create initial scope for the function
        let start_line = self.node.start_position().row as u32;
        let end_line = self.node.end_position().row as u32;
        let _scope_id = workspace
            .inspect_tool_mut()
            .create_initial_scope(self.file_uri.clone(), start_line, end_line);

        // Collect type information for each position
        let mut type_infos = Vec::new();
        
        for (line, character) in type_positions.iter() {
            // Use LSP hover directly for type information
            // (InspectTool is mainly for navigating to definitions, not hover info)
            let lsp_client = workspace.lsp_client();
            match lsp_client
                .hover(
                    TextDocumentIdentifier {
                        uri: self.file_uri.clone(),
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
    async fn send_to_llm(&self, workspace: &Workspace, prompt: String) -> Result<String> {
        let llm_client = workspace.llm_client();
        
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
