pub mod checksum;
pub mod error;
pub mod target;
pub mod target_map;

use crate::core::{MantraError, Result};
use std::path::Path;
use tree_sitter::{Parser, Tree};

/// Go language parser using tree-sitter
pub struct GoParser {
    parser: Parser,
}

impl GoParser {
    /// Create a new Go parser
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::language())
            .map_err(|e| MantraError::tree_sitter(format!("Failed to set Go language: {}", e)))?;
        Ok(Self { parser })
    }

    /// Parse Go source code
    pub fn parse(&mut self, source: &str) -> Result<Tree> {
        self.parser
            .parse(source, None)
            .ok_or_else(|| MantraError::parse("Failed to parse Go source code"))
    }

    /// Parse Go source code with optional old tree for incremental parsing
    pub fn parse_incremental(&mut self, source: &str, old_tree: Option<&Tree>) -> Result<Tree> {
        self.parser
            .parse(source, old_tree)
            .ok_or_else(|| MantraError::parse("Failed to parse Go source code"))
    }

    /// Parse a Go file and extract targets
    pub fn parse_file(&mut self, path: &Path) -> Result<target::FileInfo> {
        let source = std::fs::read_to_string(path).map_err(|e| {
            MantraError::parse(format!("Failed to read file {}: {}", path.display(), e))
        })?;

        let tree = self.parse(&source)?;

        // Extract file information
        let file_info = target::extract_file_info(path, &source, tree)?;

        Ok(file_info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = GoParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_go() {
        let mut parser = GoParser::new().unwrap();
        let source = r#"
package main

func main() {
    println("Hello, world!")
}
"#;
        let tree = parser.parse(source);
        assert!(tree.is_ok());
    }
}
