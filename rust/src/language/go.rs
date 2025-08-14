use super::LanguageSupport;

/// Go language support implementation
#[derive(Default)]
pub struct Go;

impl Go {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageSupport for Go {
    fn extract_instruction(&self, comment_text: &str) -> Option<String> {
        let mut lines = Vec::new();
        let mut in_mantra = false;

        for line in comment_text.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("// mantra:") {
                // Start of mantra comment
                in_mantra = true;
                if let Some(content) = trimmed.strip_prefix("// mantra:") {
                    let content = content.trim();
                    if !content.is_empty() {
                        lines.push(content);
                    }
                }
            } else if in_mantra && trimmed.starts_with("//") {
                // Continuation of mantra comment
                let content = trimmed.strip_prefix("//").unwrap_or("").trim();
                if !content.is_empty() {
                    lines.push(content);
                } else {
                    // Empty comment line ends the mantra instruction
                    break;
                }
            } else {
                // Non-comment line ends the mantra instruction
                break;
            }
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_instruction_single_line() {
        let go = Go::new();

        assert_eq!(
            go.extract_instruction("// mantra: Generate hello world"),
            Some("Generate hello world".to_string())
        );

        assert_eq!(go.extract_instruction("// regular comment"), None);
    }

    #[test]
    fn test_extract_instruction_multi_line() {
        let go = Go::new();

        let comment = "// mantra: Generate a function that\n// processes user data and\n// returns the result";
        assert_eq!(
            go.extract_instruction(comment),
            Some(
                "Generate a function that\nprocesses user data and\nreturns the result".to_string()
            )
        );
    }

    #[test]
    fn test_extract_instruction_with_empty_lines() {
        let go = Go::new();

        let comment = "// mantra: First line\n//\n// This should not be included";
        assert_eq!(
            go.extract_instruction(comment),
            Some("First line".to_string())
        );
    }
}
