use anyhow::{Context, Result};
use lsp_types::{Position as LspPosition, Range as LspRange, TextDocumentContentChangeEvent};

use crate::editor::crdt::{CrdtEditor, CrdtOperation};

/// Adapter to convert LSP text changes to CRDT operations
pub struct LspCrdtAdapter;

impl LspCrdtAdapter {
    /// Convert LSP text change event to CRDT operations
    pub fn lsp_change_to_crdt_ops(
        editor: &mut CrdtEditor,
        change: &TextDocumentContentChangeEvent,
    ) -> Result<Vec<CrdtOperation>> {
        let mut operations = Vec::new();

        match change.range {
            Some(range) => {
                // Convert LSP positions to byte offsets
                let start_byte = Self::lsp_position_to_byte(editor, &range.start)?;
                let end_byte = Self::lsp_position_to_byte(editor, &range.end)?;

                // If there's a deletion, create delete operation
                if start_byte < end_byte {
                    let deletion = editor
                        .delete(start_byte, end_byte)
                        .context("Failed to create deletion")?;
                    operations.push(CrdtOperation::Delete(deletion));
                }

                // If there's text to insert, create insert operation
                if !change.text.is_empty() {
                    let insertion = editor
                        .insert(start_byte, &change.text)
                        .context("Failed to create insertion")?;
                    operations.push(CrdtOperation::Insert {
                        insertion,
                        text: change.text.clone(),
                    });
                }
            }
            None => {
                // Full document replacement
                let current_len = editor.len();

                // Delete all current content
                if current_len > 0 {
                    let deletion = editor
                        .delete(0, current_len)
                        .context("Failed to delete current content")?;
                    operations.push(CrdtOperation::Delete(deletion));
                }

                // Insert new content
                if !change.text.is_empty() {
                    let insertion = editor
                        .insert(0, &change.text)
                        .context("Failed to insert new content")?;
                    operations.push(CrdtOperation::Insert {
                        insertion,
                        text: change.text.clone(),
                    });
                }
            }
        }

        Ok(operations)
    }

    /// Convert LSP position to byte offset
    fn lsp_position_to_byte(editor: &CrdtEditor, position: &LspPosition) -> Result<usize> {
        // LSP uses 0-based line and character positions
        let line = position.line as usize;
        let character = position.character as usize;

        // Convert to byte offset
        let byte_offset = editor.line_col_to_byte(line, character);
        Ok(byte_offset)
    }

    /// Convert byte offset to LSP position
    pub fn byte_to_lsp_position(editor: &CrdtEditor, byte_offset: usize) -> LspPosition {
        let (line, col) = editor.byte_to_line_col(byte_offset);
        LspPosition {
            line: line as u32,
            character: col as u32,
        }
    }

    /// Convert CRDT operation to LSP text edit
    pub fn crdt_op_to_lsp_edit(
        editor: &CrdtEditor,
        operation: &CrdtOperation,
    ) -> Result<TextDocumentContentChangeEvent> {
        match operation {
            CrdtOperation::Insert { text, .. } => {
                // For insertion, we need to determine where the text was inserted
                // This is simplified - in practice, we'd need to track the exact position
                Ok(TextDocumentContentChangeEvent {
                    range: None, // Full document update for simplicity
                    range_length: None,
                    text: text.clone(),
                })
            }
            CrdtOperation::Delete(_) => {
                // For deletion, we need to determine what was deleted
                // This is simplified - in practice, we'd need to track the exact range
                Ok(TextDocumentContentChangeEvent {
                    range: None, // Full document update for simplicity
                    range_length: None,
                    text: editor.get_text().to_string(),
                })
            }
        }
    }

    /// Apply multiple LSP changes to the editor
    pub fn apply_lsp_changes(
        editor: &mut CrdtEditor,
        changes: &[TextDocumentContentChangeEvent],
    ) -> Result<Vec<CrdtOperation>> {
        let mut all_operations = Vec::new();

        for change in changes {
            let operations = Self::lsp_change_to_crdt_ops(editor, change)?;
            all_operations.extend(operations);
        }

        Ok(all_operations)
    }

    /// Convert a range in the text to an LSP range
    pub fn text_range_to_lsp(editor: &CrdtEditor, start_byte: usize, end_byte: usize) -> LspRange {
        let start = Self::byte_to_lsp_position(editor, start_byte);
        let end = Self::byte_to_lsp_position(editor, end_byte);
        LspRange { start, end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_insertion() {
        let mut editor = CrdtEditor::from_text("Hello world");

        // Create an LSP change event for inserting ", beautiful" after "Hello"
        let change = TextDocumentContentChangeEvent {
            range: Some(LspRange {
                start: LspPosition {
                    line: 0,
                    character: 5,
                },
                end: LspPosition {
                    line: 0,
                    character: 5,
                },
            }),
            range_length: None,
            text: ", beautiful".to_string(),
        };

        let operations = LspCrdtAdapter::lsp_change_to_crdt_ops(&mut editor, &change).unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(editor.get_text(), "Hello, beautiful world");
    }

    #[test]
    fn test_lsp_deletion() {
        let mut editor = CrdtEditor::from_text("Hello, world!");

        // Delete ", world"
        let change = TextDocumentContentChangeEvent {
            range: Some(LspRange {
                start: LspPosition {
                    line: 0,
                    character: 5,
                },
                end: LspPosition {
                    line: 0,
                    character: 12,
                },
            }),
            range_length: Some(7),
            text: "".to_string(),
        };

        let operations = LspCrdtAdapter::lsp_change_to_crdt_ops(&mut editor, &change).unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(editor.get_text(), "Hello!");
    }

    #[test]
    fn test_lsp_replacement() {
        let mut editor = CrdtEditor::from_text("Hello world");

        // Replace "world" with "Rust"
        let change = TextDocumentContentChangeEvent {
            range: Some(LspRange {
                start: LspPosition {
                    line: 0,
                    character: 6,
                },
                end: LspPosition {
                    line: 0,
                    character: 11,
                },
            }),
            range_length: Some(5),
            text: "Rust".to_string(),
        };

        let operations = LspCrdtAdapter::lsp_change_to_crdt_ops(&mut editor, &change).unwrap();
        assert_eq!(operations.len(), 2); // Delete + Insert
        assert_eq!(editor.get_text(), "Hello Rust");
    }

    #[test]
    fn test_full_document_replacement() {
        let mut editor = CrdtEditor::from_text("Old content");

        // Full document replacement
        let change = TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "New content".to_string(),
        };

        let operations = LspCrdtAdapter::lsp_change_to_crdt_ops(&mut editor, &change).unwrap();
        assert_eq!(operations.len(), 2); // Delete all + Insert new
        assert_eq!(editor.get_text(), "New content");
    }

    #[test]
    fn test_position_conversion() {
        let editor = CrdtEditor::from_text("Line 1\nLine 2\nLine 3");

        // Test LSP position to byte
        let lsp_pos = LspPosition {
            line: 1,
            character: 5,
        };
        let byte_offset = LspCrdtAdapter::lsp_position_to_byte(&editor, &lsp_pos).unwrap();
        assert_eq!(byte_offset, 12); // "Line 1\nLine " = 7 + 5 = 12

        // Test byte to LSP position
        let lsp_pos_back = LspCrdtAdapter::byte_to_lsp_position(&editor, byte_offset);
        assert_eq!(lsp_pos_back.line, 1);
        assert_eq!(lsp_pos_back.character, 5);
    }

    #[test]
    fn test_multiple_changes() {
        let mut editor = CrdtEditor::from_text("Hello world");

        let changes = vec![
            // Insert " beautiful" after "Hello"
            TextDocumentContentChangeEvent {
                range: Some(LspRange {
                    start: LspPosition {
                        line: 0,
                        character: 5,
                    },
                    end: LspPosition {
                        line: 0,
                        character: 5,
                    },
                }),
                range_length: None,
                text: " beautiful".to_string(),
            },
            // Add "!" at the end
            TextDocumentContentChangeEvent {
                range: Some(LspRange {
                    start: LspPosition {
                        line: 0,
                        character: 21,
                    },
                    end: LspPosition {
                        line: 0,
                        character: 21,
                    },
                }),
                range_length: None,
                text: "!".to_string(),
            },
        ];

        let operations = LspCrdtAdapter::apply_lsp_changes(&mut editor, &changes).unwrap();
        assert_eq!(operations.len(), 2);
        assert_eq!(editor.get_text(), "Hello beautiful world!");
    }
}
