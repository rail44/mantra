pub mod checksum;
pub mod error;
pub mod target;

use crate::core::{MantraError, Result};
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
            .set_language(&tree_sitter_go::LANGUAGE.into())
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

    /// Parse using a callback to read text chunks
    pub fn parse_with_callback<T, F>(
        &mut self,
        mut callback: F,
        old_tree: Option<&Tree>,
    ) -> Result<Tree>
    where
        T: AsRef<[u8]>,
        F: FnMut(usize, tree_sitter::Point) -> T,
    {
        self.parser
            .parse_with_options(&mut callback, old_tree, None)
            .ok_or_else(|| MantraError::parse("Failed to parse Go source code"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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

    #[test]
    fn test_parse_invalid_go() {
        let mut parser = GoParser::new().unwrap();
        let source = "this is not valid go code {{{";
        // Tree-sitter still returns a tree even for invalid code
        let tree = parser.parse(source);
        assert!(tree.is_ok());
        // But the tree will have errors
        let tree = tree.unwrap();
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_incremental_parsing() {
        let mut parser = GoParser::new().unwrap();

        // Initial parse
        let source1 = "package main\n\nfunc foo() {}";
        let tree1 = parser.parse(source1).unwrap();

        // Incremental parse with changes
        let source2 = "package main\n\nfunc foo() {}\nfunc bar() {}";
        let tree2 = parser.parse_incremental(source2, Some(&tree1)).unwrap();

        assert!(!tree2.root_node().has_error());
    }
}
