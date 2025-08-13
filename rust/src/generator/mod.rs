use anyhow::{Context, Result};
use std::path::Path;

use crate::config::Config;
use crate::llm::{LLMClient, Message, CompletionRequest};
use crate::parser::{GoParser, target::Target};

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
        let file_info = parser.parse_file(file_path)?;
        
        tracing::info!("Found {} targets in file", file_info.targets.len());
        for target in &file_info.targets {
            tracing::info!("  - {} ({})", target.name, target.instruction);
        }
        
        // Process each target
        let mut modified_source = file_info.source_content.clone();
        
        for target in &file_info.targets {
            // Generate code for this target
            let generated_code = self.generate_target(target, &file_info.package_name).await?;
            
            // Replace the function body in the source
            modified_source = self.replace_function_body(
                &modified_source,
                target,
                &generated_code
            )?;
        }
        
        Ok(modified_source)
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
    
    /// Replace a function body in the source code
    fn replace_function_body(&self, source: &str, target: &Target, new_body: &str) -> Result<String> {
        // Find the function and replace its body
        let lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();
        let mut result = Vec::new();
        let mut i = 0;
        
        while i < lines.len() {
            let line = &lines[i];
            
            // Check if this is the target function
            if line.contains(&target.signature) {
                // Add the function signature
                result.push(format!("{} {}", target.signature, new_body));
                
                // Skip the old function body
                i += 1;
                let mut brace_count = 0;
                let mut started = false;
                
                while i < lines.len() {
                    let line = &lines[i];
                    for ch in line.chars() {
                        if ch == '{' {
                            brace_count += 1;
                            started = true;
                        } else if ch == '}' {
                            brace_count -= 1;
                        }
                    }
                    
                    if started && brace_count == 0 {
                        break;
                    }
                    i += 1;
                }
            } else {
                result.push(line.clone());
            }
            i += 1;
        }
        
        Ok(result.join("\n"))
    }
}