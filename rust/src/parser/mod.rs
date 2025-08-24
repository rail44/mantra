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

    /// Parse a Go file and extract targets
    pub fn parse_file(&mut self, path: &Path) -> Result<target::FileInfo> {
        let timer =
            crate::core::metrics::Timer::start_debug(format!("parse_file:{}", path.display()));

        let source = std::fs::read_to_string(path).map_err(|e| {
            MantraError::parse(format!("Failed to read file {}: {}", path.display(), e))
        })?;

        let tree = self.parse(&source)?;

        // Extract file information
        let file_info = target::extract_file_info(path, &source, tree)?;

        timer.stop_with_message(&format!("Found {} targets", file_info.targets.len()));
        Ok(file_info)
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

    #[test]
    fn test_parse_file() {
        let mut parser = GoParser::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.go");

        let source = r#"
package main

import "fmt"

// mantra: Say hello
func SayHello(name string) {
    panic("not implemented")
}

func helper() {
    // This function has no mantra comment
}
"#;

        fs::write(&file_path, source).unwrap();

        let file_info = parser.parse_file(&file_path).unwrap();

        assert_eq!(file_info.package_name, "main");
        assert_eq!(file_info.imports.len(), 1);
        assert_eq!(file_info.imports[0].path, "fmt");
        assert_eq!(file_info.targets.len(), 1);
        assert_eq!(file_info.targets[0].name, "SayHello");
        assert_eq!(file_info.targets[0].instruction, "Say hello");
    }

    #[test]
    fn test_parse_file_not_found() {
        let mut parser = GoParser::new().unwrap();
        let result = parser.parse_file(Path::new("/nonexistent/file.go"));
        assert!(result.is_err());
        // Just verify it's an error - the specific message may vary
    }
}
