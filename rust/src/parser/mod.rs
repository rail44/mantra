pub mod target;
pub mod checksum;
pub mod editor;

use anyhow::{Context, Result};
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
            .context("Failed to set Go language for parser")?;
        Ok(Self { parser })
    }
    
    /// Parse Go source code
    pub fn parse(&mut self, source: &str) -> Result<Tree> {
        self.parser
            .parse(source, None)
            .context("Failed to parse Go source code")
    }
    
    /// Parse a Go file and extract targets
    pub fn parse_file(&mut self, path: &Path) -> Result<target::FileInfo> {
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        
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