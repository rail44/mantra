use crate::parser::target::Target;

/// Build a prompt for generating Go code implementation
pub fn build_prompt(target: &Target) -> String {
    format!(
        "Generate the Go implementation for this function:\n\n\
         Function signature: {}\n\
         Instruction: {}\n\n\
         Return only the code that goes inside the function body (without the curly braces).\n\
         For example, if the function should add two numbers, just return: return a + b",
        target.signature,
        if target.instruction.is_empty() {
            "Implement this function"
        } else {
            &target.instruction
        }
    )
}

/// Clean generated code by removing markdown formatting and extra whitespace
pub fn clean_generated_code(code: String) -> String {
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
    fn test_build_prompt() {
        let target = Target {
            name: "Add".to_string(),
            signature: "func Add(a, b int) int".to_string(),
            instruction: "Add two numbers and return the result".to_string(),
        };

        let prompt = build_prompt(&target);
        assert!(prompt.contains("func Add(a, b int) int"));
        assert!(prompt.contains("Add two numbers and return the result"));
    }

    #[test]
    fn test_clean_generated_code() {
        let code_with_markdown = "```go\nreturn a + b\n```".to_string();
        let cleaned = clean_generated_code(code_with_markdown);
        assert_eq!(cleaned, "return a + b");

        let code_with_backticks = "```\nreturn a + b\n```".to_string();
        let cleaned = clean_generated_code(code_with_backticks);
        assert_eq!(cleaned, "return a + b");

        let plain_code = "  return a + b  ".to_string();
        let cleaned = clean_generated_code(plain_code);
        assert_eq!(cleaned, "return a + b");
    }
}
