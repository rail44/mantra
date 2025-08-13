use anyhow::{Context, Result};
use std::path::Path;

use crate::config::Config;
use crate::llm::{LLMClient, Message, CompletionRequest};
use crate::parser::{GoParser, target::Target, checksum::calculate_checksum, editor::TreeEditor};

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
        
        // Create tree editor
        let mut editor = TreeEditor::new(source, tree);
        
        // Process each target
        for target in &file_info.targets {
            // Calculate checksum
            let checksum = calculate_checksum(target);
            
            // Generate code for this target
            let generated_code = self.generate_target(target, &file_info.package_name).await?;
            
            // Find the function info and replace
            if let Some(func_info) = editor.find_function_info(&target.signature) {
                // Replace the function body using tree-sitter
                editor.replace_function_body(&func_info, &generated_code, checksum)?;
            } else {
                tracing::warn!("Could not find function node for: {}", target.signature);
            }
        }
        
        // Apply all edits and return
        Ok(editor.apply_edits())
    }
    
    /// Generate code for a single target function
    async fn generate_target(&self, target: &Target, package_name: &str) -> Result<String> {
        // Build the prompt
        let prompt = self.build_prompt(target, package_name);
        
        tracing::debug!("Generating for target: {}", target.name);
        tracing::debug!("Prompt: {}", prompt);
        
        // Create the request
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: vec![
                Message::system("You are a Go code generator. Generate only the function body implementation. Do not include the function signature. Do not include explanations or markdown code blocks."),
                Message::user(prompt),
            ],
            temperature: 0.2,
            max_tokens: Some(1000),
        };
        
        // Send to LLM
        let response = if self.config.url.contains("openrouter") {
            self.client.complete_openrouter(request).await?
        } else {
            self.client.complete(request).await?
        };
        
        // Extract the generated code
        let generated = response.choices
            .first()
            .map(|c| c.message.content.clone())
            .context("No response from LLM")?;
        
        tracing::debug!("Raw LLM response: {}", generated);
        
        // Clean up the response (remove markdown if present)
        let cleaned = self.clean_generated_code(generated);
        tracing::debug!("Cleaned response: {}", cleaned);
        
        Ok(cleaned)
    }
    
    /// Build a prompt for the LLM
    fn build_prompt(&self, target: &Target, package_name: &str) -> String {
        format!(
            "Generate the Go implementation for this function:\n\n\
             Package: {}\n\
             Function signature: {}\n\
             Instruction: {}\n\n\
             Generate only the function body, starting with '{{' and ending with '}}'.",
            package_name,
            target.signature,
            target.instruction
        )
    }
    
    /// Clean up generated code (remove markdown blocks, etc.)
    fn clean_generated_code(&self, code: String) -> String {
        let code = code.trim();
        
        // Remove markdown code blocks if present
        if code.starts_with("```go") || code.starts_with("```") {
            let lines: Vec<&str> = code.lines().collect();
            let start = 1; // Skip the opening ```
            let end = lines.len().saturating_sub(1); // Skip the closing ```
            
            if end > start {
                return lines[start..end].join("\n");
            }
        }
        
        code.to_string()
    }
    
}