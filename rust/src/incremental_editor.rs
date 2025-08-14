use anyhow::Result;
use std::path::Path;
use tree_sitter::{InputEdit, Point, Tree};

use crate::generator::edit_event::EditEvent;
use crate::parser::{target_map::TargetMap, GoParser};

/// Manages incremental editing of source code with Tree-sitter
pub struct IncrementalEditor {
    /// Current source code
    source: String,
    /// Current parse tree
    tree: Tree,
    /// Parser instance
    parser: GoParser,
}

impl IncrementalEditor {
    /// Create a new incremental editor from source file
    pub fn new(file_path: &Path) -> Result<Self> {
        let source = std::fs::read_to_string(file_path)?;
        let mut parser = GoParser::new()?;
        let tree = parser.parse(&source)?;

        Ok(Self {
            source,
            tree,
            parser,
        })
    }

    /// Get current source
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get current tree
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Apply an EditEvent to the source and update the tree incrementally
    pub fn apply_edit_event(
        &mut self,
        event: EditEvent,
        func_start_byte: usize,
        has_body: bool,
        body_start: usize,
        body_end: usize,
    ) -> Result<()> {
        let mut edits_applied = false;

        // Check if there's already a checksum comment
        let lines: Vec<&str> = self.source.lines().collect();
        let func_start_point = self.byte_to_point(func_start_byte);
        let has_checksum = func_start_point.row > 0
            && lines
                .get((func_start_point.row - 1) as usize)
                .map(|line| line.contains("// mantra:checksum:"))
                .unwrap_or(false);

        // Track offset change if we add a checksum comment
        let mut byte_offset = 0;

        if !has_checksum && func_start_point.row > 0 {
            // Find the start of the line with the function
            let line_start_byte = self.position_to_byte(func_start_point.row as u32, 0);

            // Insert checksum comment before the function
            let checksum_comment = format!("// mantra:checksum:{:x}\n", event.checksum);
            byte_offset = checksum_comment.len();
            let checksum_edit = Edit::new(line_start_byte, line_start_byte, checksum_comment);

            // Apply checksum edit
            let input_edit = self.calculate_input_edit(&checksum_edit)?;
            self.apply_edit_to_source(&checksum_edit);
            self.tree.edit(&input_edit);
            edits_applied = true;
        }

        // Replace the function body if it exists
        if has_body {
            let adjusted_body_start = body_start + byte_offset;
            let adjusted_body_end = body_end + byte_offset;

            // Format the new body with proper indentation
            let formatted_body = if event.new_body.trim().is_empty() {
                "{\n\tpanic(\"not implemented\")\n}".to_string()
            } else {
                let indented = indent_code(&event.new_body, "\t");
                format!("{{\n{}\n}}", indented)
            };

            let body_edit = Edit::new(adjusted_body_start, adjusted_body_end, formatted_body);

            // Apply body edit
            let input_edit = self.calculate_input_edit(&body_edit)?;
            self.apply_edit_to_source(&body_edit);
            self.tree.edit(&input_edit);
            edits_applied = true;
        }

        // Reparse once after all edits
        if edits_applied {
            self.tree = self
                .parser
                .parse_incremental(&self.source, Some(&self.tree))?;
        }

        Ok(())
    }

    /// Apply an edit to the source and update the tree incrementally
    pub fn apply_edit(&mut self, edit: &Edit) -> Result<()> {
        // Calculate the InputEdit for tree-sitter
        let input_edit = self.calculate_input_edit(edit)?;

        // Apply the edit to the source
        self.apply_edit_to_source(edit);

        // Update the tree incrementally
        self.tree.edit(&input_edit);

        // Reparse with the edited tree for incremental parsing
        let new_tree = self
            .parser
            .parse_incremental(&self.source, Some(&self.tree))?;
        self.tree = new_tree;

        Ok(())
    }

    /// Calculate InputEdit from our Edit structure
    fn calculate_input_edit(&self, edit: &Edit) -> Result<InputEdit> {
        // Convert byte positions to Point (row, column)
        let start_point = self.byte_to_point(edit.start_byte);
        let old_end_point = self.byte_to_point(edit.end_byte);

        // Calculate new end position after the edit
        let new_text_lines: Vec<&str> = edit.new_text.lines().collect();
        let new_end_byte = edit.start_byte + edit.new_text.len();

        let new_end_point = if new_text_lines.len() > 1 {
            // Multi-line replacement
            Point::new(
                start_point.row + new_text_lines.len() - 1,
                new_text_lines.last().map(|s| s.len()).unwrap_or(0),
            )
        } else {
            // Single line replacement
            Point::new(start_point.row, start_point.column + edit.new_text.len())
        };

        Ok(InputEdit {
            start_byte: edit.start_byte,
            old_end_byte: edit.end_byte,
            new_end_byte,
            start_position: start_point,
            old_end_position: old_end_point,
            new_end_position: new_end_point,
        })
    }

    /// Apply edit to source string
    fn apply_edit_to_source(&mut self, edit: &Edit) {
        let before = &self.source[..edit.start_byte];
        let after = &self.source[edit.end_byte..];
        self.source = format!("{}{}{}", before, edit.new_text, after);
    }

    /// Convert byte position to Point (row, column)
    fn byte_to_point(&self, byte_pos: usize) -> Point {
        let mut row = 0;
        let mut col = 0;
        let mut current_byte = 0;

        for ch in self.source.chars() {
            if current_byte >= byte_pos {
                break;
            }

            if ch == '\n' {
                row += 1;
                col = 0;
            } else {
                col += ch.len_utf8();
            }

            current_byte += ch.len_utf8();
        }

        Point::new(row, col)
    }

    /// Convert Position (line, character) to byte offset
    fn position_to_byte(&self, line: u32, character: u32) -> usize {
        let mut current_line = 0;
        let mut byte_offset = 0;
        let mut line_start_byte = 0;

        for ch in self.source.chars() {
            if current_line == line {
                // We're at the target line
                if (byte_offset - line_start_byte) >= character as usize {
                    return byte_offset;
                }
            }

            if ch == '\n' {
                current_line += 1;
                line_start_byte = byte_offset + ch.len_utf8();
            }

            byte_offset += ch.len_utf8();

            if current_line > line {
                break;
            }
        }

        // Return the position at the target line and character
        line_start_byte + character as usize
    }

    /// Build a new TargetMap from the current state
    pub fn build_target_map(&self) -> Result<TargetMap<'_>> {
        TargetMap::build(&self.tree, &self.source)
    }
}

/// Represents an edit operation
#[derive(Debug, Clone)]
pub struct Edit {
    /// Start byte position of the edit
    pub start_byte: usize,
    /// End byte position of the edit (exclusive)
    pub end_byte: usize,
    /// New text to insert
    pub new_text: String,
}

impl Edit {
    pub fn new(start_byte: usize, end_byte: usize, new_text: String) -> Self {
        Self {
            start_byte,
            end_byte,
            new_text,
        }
    }

    /// Create an edit to replace a function body
    pub fn replace_function_body(node: &tree_sitter::Node, new_body: &str) -> Option<Self> {
        // Get the body node
        let body_node = node.child_by_field_name("body")?;

        // Format the new body with proper indentation
        let formatted_body = if new_body.trim().is_empty() {
            "{\n\tpanic(\"not implemented\")\n}".to_string()
        } else {
            let indented = indent_code(new_body, "\t");
            format!("{{\n{}\n}}", indented)
        };

        Some(Self::new(
            body_node.start_byte(),
            body_node.end_byte(),
            formatted_body,
        ))
    }
}

/// Indent code with given prefix
fn indent_code(code: &str, indent: &str) -> String {
    let code = code.trim();
    let code = if code.starts_with('{') && code.ends_with('}') {
        &code[1..code.len() - 1]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_incremental_edit() -> Result<()> {
        // Create a temporary file
        let dir = tempdir()?;
        let file_path = dir.path().join("test.go");
        let mut file = File::create(&file_path)?;
        writeln!(
            file,
            r#"package main

// mantra: Add two numbers
func Add(a, b int) int {{
    panic("not implemented")
}}"#
        )?;

        // Create editor
        let mut editor = IncrementalEditor::new(&file_path)?;

        // Get edit from initial target map
        let edit = {
            let target_map = editor.build_target_map()?;
            assert_eq!(target_map.len(), 1);

            // Get the target and its node
            let checksum = target_map.checksums().next().unwrap();
            let (_, node) = target_map.get(checksum).unwrap();

            // Create an edit to replace the function body
            Edit::replace_function_body(node, "return a + b").unwrap()
        };

        // Apply the edit
        editor.apply_edit(&edit)?;

        // Verify the source was updated
        assert!(editor.source().contains("return a + b"));
        assert!(!editor.source().contains("panic(\"not implemented\")"));

        Ok(())
    }
}
