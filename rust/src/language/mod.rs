pub mod go;

/// Language-specific instruction extraction
pub trait LanguageSupport: Send + Sync {
    /// Extract instruction from comment text
    /// Returns None if the comment is not a mantra instruction
    fn extract_instruction(&self, comment_text: &str) -> Option<String>;
}
