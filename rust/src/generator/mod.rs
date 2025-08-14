pub mod edit_event;

use anyhow::{Context, Result};
use std::path::Path;
use tree_sitter::TreeCursor;

use self::edit_event::EditEvent;
use crate::config::Config;
use crate::editor::TextEdit;
use crate::llm::{CompletionRequest, LLMClient, Message, ProviderSpec};
use crate::lsp::{Client as LspClient, Position, TextDocumentIdentifier, TextDocumentItem};
use crate::parser::{
    checksum::calculate_checksum,
    target::{get_function_type_positions, Target},
    GoParser,
};

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

    /// Generate code for a single file
    pub async fn generate_file(&self, file_path: &Path) -> Result<String> {
        // Parse the file
        let mut parser = GoParser::new()?;
        let source = std::fs::read_to_string(file_path)?;
        let tree = parser.parse(&source)?;
        let file_info = parser.parse_file(file_path)?;

        tracing::info!("Found {} targets in file", file_info.targets.len());
        for target in &file_info.targets {
            tracing::info!("  - {} ({})", target.name, target.instruction);
        }

        // Collect edit events for all targets
        let mut events = Vec::new();

        for target in &file_info.targets {
            // Calculate checksum
            let checksum = calculate_checksum(target);

            // Generate code for this target with type information
            let generated_code = self
                .generate_target(target, &file_info.package_name, file_path, &source, &tree)
                .await?;

            // Create edit event
            events.push(EditEvent::new(
                checksum,
                target.signature.clone(),
                generated_code,
            ));
        }

        // Convert events to LSP-style edits
        let edits = edit_event::convert_to_lsp_edits(&source, &tree, events)?;

        // Apply edits to source (simple string replacement for now)
        let mut result = source.clone();

        // Sort edits by position (reverse order to apply from end to start)
        let mut sorted_edits = edits;
        sorted_edits
            .sort_by_key(|e| std::cmp::Reverse((e.range.start.line, e.range.start.character)));

        for edit in sorted_edits {
            // Simple string replacement based on line/character positions
            result = self.apply_edit_to_string(&result, &edit);
        }

        Ok(result)
    }

    /// Apply a single edit to a string
    fn apply_edit_to_string(&self, source: &str, edit: &TextEdit) -> String {
        let lines: Vec<&str> = source.lines().collect();
        let mut result = String::new();

        for (line_num, line) in lines.iter().enumerate() {
            if line_num as u32 == edit.range.start.line {
                // This is the line where the edit starts
                if line_num as u32 == edit.range.end.line {
                    // Edit is within a single line
                    result.push_str(&line[..edit.range.start.character as usize]);
                    result.push_str(&edit.new_text);
                    result.push_str(&line[edit.range.end.character as usize..]);
                } else {
                    // Edit starts on this line but ends on another
                    result.push_str(&line[..edit.range.start.character as usize]);
                    result.push_str(&edit.new_text);
                    // Skip lines until end line
                }
            } else if line_num as u32 > edit.range.start.line
                && (line_num as u32) < edit.range.end.line
            {
                // Skip lines in the middle of the edit range
                continue;
            } else if line_num as u32 == edit.range.end.line {
                // This is the line where the edit ends
                result.push_str(&line[edit.range.end.character as usize..]);
            } else {
                // Lines outside the edit range
                result.push_str(line);
            }

            if line_num < lines.len() - 1 {
                result.push('\n');
            }
        }

        result
    }

    /// Generate code for a single target function
    async fn generate_target(
        &self,
        target: &Target,
        package_name: &str,
        file_path: &Path,
        source: &str,
        tree: &tree_sitter::Tree,
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
        let type_info = match self
            .find_function_node_and_get_type_info(tree, source, target, file_path)
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

    /// Find function node in tree and get type information from LSP
    async fn find_function_node_and_get_type_info(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        target: &Target,
        file_path: &Path,
    ) -> Result<String> {
        let root_node = tree.root_node();
        let mut cursor = root_node.walk();

        // Find the matching function node
        let function_node = Self::find_function_by_name(&mut cursor, source, &target.name)?;

        // Get LSP positions for parameter and return types
        let type_positions = get_function_type_positions(&function_node);

        // Get type information for all positions
        self.collect_type_info_at_positions(file_path, source, &type_positions)
            .await
    }

    /// Find function node by name in the tree
    fn find_function_by_name<'a>(
        cursor: &mut TreeCursor<'a>,
        source: &str,
        function_name: &str,
    ) -> Result<tree_sitter::Node<'a>> {
        loop {
            let node = cursor.node();
            if node.kind() == "function_declaration" || node.kind() == "method_declaration" {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                        if name == function_name {
                            return Ok(node);
                        }
                    }
                }
            }

            if cursor.goto_first_child() {
                if let Ok(found) = Self::find_function_by_name(cursor, source, function_name) {
                    return Ok(found);
                }
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
        anyhow::bail!("Function '{}' not found in tree", function_name)
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
