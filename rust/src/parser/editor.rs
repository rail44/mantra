use anyhow::{Context, Result};
use tree_sitter::{Node, Tree, TreeCursor};

/// Represents an edit operation on the source code
#[derive(Debug, Clone)]
pub struct Edit {
    pub start_byte: usize,
    pub end_byte: usize,
    pub replacement: String,
}

/// Function info for editing
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub start_byte: usize,
    pub end_byte: usize,
    pub body_start: usize,
    pub body_end: usize,
}

/// Editor for manipulating Go source code using tree-sitter
pub struct TreeEditor {
    source: String,
    tree: Tree,
    edits: Vec<Edit>,
}

impl TreeEditor {
    /// Create a new tree editor
    pub fn new(source: String, tree: Tree) -> Self {
        Self {
            source,
            tree,
            edits: Vec::new(),
        }
    }
    
    /// Replace a function body with new implementation
    pub fn replace_function_body(&mut self, func_info: &FunctionInfo, new_body: &str, checksum: u64) -> Result<()> {
        // Format the new body with checksum
        let formatted_body = format!(
            "{{\n\t// mantra:checksum:{:x}\n{}}}",
            checksum,
            self.indent_code(new_body, "\t")
        );
        
        // Record the edit
        self.edits.push(Edit {
            start_byte: func_info.body_start,
            end_byte: func_info.body_end,
            replacement: formatted_body,
        });
        
        Ok(())
    }
    
    /// Find a function by signature and return its info
    pub fn find_function_info(&self, signature: &str) -> Option<FunctionInfo> {
        let root = self.tree.root_node();
        self.find_function_in_node(&root, signature)
            .map(|node| self.extract_function_info(&node))
    }
    
    /// Extract function info from node
    fn extract_function_info(&self, node: &Node) -> FunctionInfo {
        let body_node = node.child_by_field_name("body");
        
        FunctionInfo {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            body_start: body_node.as_ref().map(|n| n.start_byte()).unwrap_or(node.end_byte()),
            body_end: body_node.as_ref().map(|n| n.end_byte()).unwrap_or(node.end_byte()),
        }
    }
    
    fn find_function_in_node<'a>(&self, node: &Node<'a>, signature: &str) -> Option<Node<'a>> {
        // Check if this is a function with matching signature
        if node.kind() == "function_declaration" || node.kind() == "method_declaration" {
            let sig = self.build_signature(node);
            if sig == signature {
                return Some(*node);
            }
        }
        
        // Check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = self.find_function_in_node(&child, signature) {
                return Some(found);
            }
        }
        
        None
    }
    
    /// Build signature string from function node
    fn build_signature(&self, node: &Node) -> String {
        // Extract the signature text from source
        let start = node.start_byte();
        let mut end = node.end_byte();
        
        // Find the opening brace to get just the signature
        if let Some(body) = node.child_by_field_name("body") {
            end = body.start_byte();
        }
        
        self.source[start..end].trim_end().trim_end_matches('{').trim().to_string()
    }
    
    /// Apply all edits and return the modified source
    pub fn apply_edits(mut self) -> String {
        // Sort edits by start position (in reverse to apply from end to start)
        self.edits.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));
        
        let mut result = self.source;
        
        for edit in self.edits {
            result.replace_range(edit.start_byte..edit.end_byte, &edit.replacement);
        }
        
        result
    }
    
    /// Indent code with given prefix
    fn indent_code(&self, code: &str, indent: &str) -> String {
        // Remove opening and closing braces if present
        let code = code.trim();
        let code = if code.starts_with('{') && code.ends_with('}') {
            &code[1..code.len()-1]
        } else {
            code
        };
        
        code.lines()
            .map(|line| {
                if line.trim().is_empty() {
                    String::new()
                } else {
                    format!("{}{}", indent, line.trim_start())
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::GoParser;
    
    #[test]
    fn test_tree_editor() {
        let source = r#"package main

func Add(a, b int) int {
    panic("not implemented")
}"#;
        
        let mut parser = GoParser::new().unwrap();
        let tree = parser.parse(source).unwrap();
        
        let mut editor = TreeEditor::new(source.to_string(), tree);
        
        // Find the function
        let func_info = editor.find_function_info("func Add(a, b int) int").unwrap();
        
        // Replace the body
        editor.replace_function_body(&func_info, "{\n    return a + b\n}", 0x12345678).unwrap();
        
        let result = editor.apply_edits();
        assert!(result.contains("return a + b"));
        assert!(result.contains("mantra:checksum:12345678"));
    }
}