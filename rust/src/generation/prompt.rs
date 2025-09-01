use crate::parser::target::Target;
use rustc_hash::FxHashMap;
use std::fmt::Write;

/// Build a system prompt for Go code generation
pub fn build_system_prompt() -> String {
    "You are a Go code generator. Your task is to implement function bodies based on the given signatures and instructions.

IMPORTANT RULES:
1. Return ONLY the Go code that goes inside the function body
2. Do NOT include any explanations, comments, or text before or after the code
3. Do NOT include the function signature or curly braces
4. Do NOT use markdown formatting, code blocks, or backticks
5. Return just the raw Go statements that implement the function

If you need to investigate type structures, use the 'inspect' tool.

Examples:
- For a function that returns the sum: return a + b
- For a function that initializes a struct: return &MyStruct{field: value}
- For a void function that prints: fmt.Println(message)".to_string()
}

/// Build a user prompt with type definitions for generating Go code implementation
pub fn build_prompt_with_types(
    target: &Target,
    type_definitions: &FxHashMap<String, String>,
) -> String {
    let mut prompt = format!("Function signature: {}\n", target.signature);

    // Add type definitions if available
    if !type_definitions.is_empty() {
        prompt.push_str("\nAvailable type definitions:\n");
        for (scope_id, definition) in type_definitions {
            writeln!(&mut prompt, "Type {scope_id}: {definition}").unwrap();
        }
    }

    // Add instruction
    write!(
        &mut prompt,
        "\nTask: {}",
        if target.instruction.is_empty() {
            "Implement this function"
        } else {
            &target.instruction
        }
    )
    .unwrap();

    prompt
}

/// Clean generated code by removing markdown formatting and extra whitespace
pub fn clean_generated_code(code: &str) -> String {
    let mut cleaned = code.trim().to_string();

    // Remove markdown code blocks if present
    if cleaned.starts_with("```") {
        if let Some(start) = cleaned.find('\n') {
            cleaned = cleaned[start + 1..].to_string();
        }
    }
    if cleaned.ends_with("```") {
        if let Some(end) = cleaned.rfind("\n```") {
            cleaned = cleaned[..end].to_string();
        }
    }

    // Remove language identifier like ```go
    if cleaned.starts_with("go\n") {
        cleaned = cleaned[3..].to_string();
    }

    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_generated_code() {
        let code_with_markdown = "```go\nreturn a + b\n```".to_string();
        let cleaned = clean_generated_code(&code_with_markdown);
        assert_eq!(cleaned, "return a + b");

        let code_with_backticks = "```\nreturn a + b\n```".to_string();
        let cleaned = clean_generated_code(&code_with_backticks);
        assert_eq!(cleaned, "return a + b");

        let plain_code = "  return a + b  ".to_string();
        let cleaned = clean_generated_code(&plain_code);
        assert_eq!(cleaned, "return a + b");
    }
}
