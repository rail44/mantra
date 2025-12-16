pub mod ast_utils;
pub mod checksum;
pub mod error;
pub mod position_utils;
pub mod target;
pub mod type_collector;

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
            .map_err(|e| MantraError::tree_sitter(format!("Failed to set Go language: {e}")))?;
        Ok(Self { parser })
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
