use anyhow::Result;
use cola::{Replica, ReplicaId};
use crop::Rope;
use lsp_types::{Position, Range, TextDocumentContentChangeEvent, TextEdit};
use tree_sitter::Tree;

use crate::parser::GoParser;

/// Result of a deletion operation
#[derive(Debug)]
pub struct DeletionResult {
    pub byte_start: usize,
    pub byte_end: usize,
    pub lsp_range: Range,
}

/// Result of an insertion operation
#[derive(Debug)]
pub struct InsertionResult {
    pub byte_pos: usize,
    pub lsp_pos: Position,
}

/// Snapshot of text state for CRDT operations
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// The cola replica for this snapshot
    pub(crate) replica: Replica,
    /// The text content using crop's Rope for efficient editing
    pub(crate) rope: Rope,
    /// Document version for LSP synchronization
    pub(crate) version: i32,
}

impl Snapshot {
    /// Fork this snapshot with a new replica ID
    pub fn fork(&self) -> Self {
        Snapshot {
            replica: self.replica.fork(fastrand::u64(..) as ReplicaId),
            rope: self.rope.clone(),
            version: self.version,
        }
    }

    /// Convert byte position to LSP position
    pub fn byte_to_lsp_position(&self, byte_pos: usize) -> Position {
        let line = self.rope.line_of_byte(byte_pos);
        let line_start_byte = self.rope.byte_of_line(line);

        // Convert byte offset within line to UTF-16 character offset
        let byte_offset = byte_pos - line_start_byte;
        let line_start_utf16 = self.rope.utf16_code_unit_of_byte(line_start_byte);
        let target_utf16 = self
            .rope
            .utf16_code_unit_of_byte(line_start_byte + byte_offset);
        let utf16_col = target_utf16 - line_start_utf16;

        Position {
            line: line as u32,
            character: utf16_col as u32,
        }
    }

    /// Apply deletion and return the deletion result with LSP positions
    pub fn apply_deletion(
        &mut self,
        edit_snapshot: &mut Snapshot,
        start: usize,
        end: usize,
    ) -> Option<DeletionResult> {
        let deletion = edit_snapshot.replica.deleted(start..end);
        let ranges = self.replica.integrate_deletion(&deletion);

        if ranges.is_empty() {
            return None;
        }

        // Safe to unwrap because we checked ranges is not empty
        let first_range = ranges.first().expect("ranges should not be empty");
        let last_range = ranges.last().expect("ranges should not be empty");
        let byte_start = first_range.start;
        let byte_end = last_range.end;

        // Calculate LSP positions before deletion
        let start_pos = self.byte_to_lsp_position(byte_start);
        let end_pos = self.byte_to_lsp_position(byte_end);

        // Apply deletions to rope in reverse order
        for range in ranges.iter().rev() {
            self.rope.delete(range.clone());
        }

        Some(DeletionResult {
            byte_start,
            byte_end,
            lsp_range: Range::new(start_pos, end_pos),
        })
    }

    /// Apply insertion and return the insertion result with LSP position
    pub fn apply_insertion(
        &mut self,
        edit_snapshot: &mut Snapshot,
        position: usize,
        text: &str,
    ) -> Option<InsertionResult> {
        let insertion = edit_snapshot.replica.inserted(position, text.len());

        if let Some(actual_pos) = self.replica.integrate_insertion(&insertion) {
            // Calculate LSP position before insertion (for consistency, though position doesn't change for insertion)
            let lsp_pos = self.byte_to_lsp_position(actual_pos);

            self.rope.insert(actual_pos, text);

            Some(InsertionResult {
                byte_pos: actual_pos,
                lsp_pos,
            })
        } else {
            None
        }
    }
}

/// CRDT-based collaborative text editor with integrated parsing
pub struct CrdtEditor {
    /// Current text state
    snapshot: Snapshot,
    /// Go parser for maintaining AST
    parser: GoParser,
    /// Current syntax tree (None before first parse)
    tree: Option<Tree>,
}

impl CrdtEditor {
    /// Create a new CRDT editor with integrated parser
    pub fn new(initial_text: &str) -> Result<Self> {
        let snapshot = Snapshot {
            replica: Replica::new(Self::generate_replica_id(), initial_text.len()),
            rope: Rope::from(initial_text),
            version: 0,
        };

        let parser = GoParser::new()?;

        let mut editor = Self {
            snapshot,
            parser,
            tree: None,
        };

        editor.reparse()?; // Initial parse
        Ok(editor)
    }

    /// Generate a unique replica ID using fast random number generation
    fn generate_replica_id() -> ReplicaId {
        // Use fastrand for fast, non-cryptographic random number generation
        fastrand::u64(..) as ReplicaId
    }

    /// Get the current text content
    pub fn get_text(&self) -> String {
        self.snapshot.rope.to_string()
    }

    /// Get a String for a byte range without allocating the full document
    pub fn get_text_range(&self, start: usize, end: usize) -> String {
        self.snapshot.rope.byte_slice(start..end).to_string()
    }

    /// Get a reference to the internal rope for efficient text access
    pub fn rope(&self) -> &Rope {
        &self.snapshot.rope
    }

    /// Get the current syntax tree
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Re-parse the current snapshot
    fn reparse(&mut self) -> Result<()> {
        self.tree = Some(
            self.parser
                .parse_with_callback(
                    |byte_offset, _position| {
                        if byte_offset >= self.snapshot.rope.byte_len() {
                            return "";
                        }
                        self.snapshot
                            .rope
                            .byte_slice(byte_offset..)
                            .chunks()
                            .next()
                            .unwrap_or("")
                    },
                    None, // Currentrly, full re-parse (no incremental parsing)
                )
                .map_err(|e| anyhow::anyhow!("Failed to parse: {}", e))?,
        );
        Ok(())
    }

    fn lsp_position_to_byte_with_rope(position: Position, rope: &Rope) -> usize {
        let line_start_byte = rope.byte_of_line(position.line as usize);
        let line_start_utf16 = rope.utf16_code_unit_of_byte(line_start_byte);

        // Calculate the target UTF-16 position
        let target_utf16 = line_start_utf16 + position.character as usize;

        // Convert to byte position
        rope.byte_of_utf16_code_unit(target_utf16)
    }

    /// Convert byte position to LSP position
    pub fn byte_to_lsp_position(&self, byte_pos: usize) -> Position {
        let line = self.snapshot.rope.line_of_byte(byte_pos);
        let line_start_byte = self.snapshot.rope.byte_of_line(line);

        // Convert byte offset within line to UTF-16 character offset
        let byte_offset = byte_pos - line_start_byte;
        let line_start_utf16 = self.snapshot.rope.utf16_code_unit_of_byte(line_start_byte);
        let target_utf16 = self
            .snapshot
            .rope
            .utf16_code_unit_of_byte(line_start_byte + byte_offset);
        let utf16_col = target_utf16 - line_start_utf16;

        Position {
            line: line as u32,
            character: utf16_col as u32,
        }
    }

    /// Convert line/column to byte position
    pub fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        let line_start = self.snapshot.rope.byte_of_line(line);
        line_start + col.min(self.snapshot.rope.line(line).byte_len())
    }

    /// Get the current document version
    pub fn get_version(&self) -> i32 {
        self.snapshot.version
    }

    /// Increment the document version and return the new version
    fn increment_version(&mut self) -> i32 {
        self.snapshot.version += 1;
        self.snapshot.version
    }

    /// Create a snapshot of the current state
    pub fn fork(&self) -> Snapshot {
        Snapshot {
            replica: self.snapshot.replica.fork(Self::generate_replica_id()),
            rope: self.snapshot.rope.clone(),
            version: self.snapshot.version,
        }
    }

    /// Internal byte edit without version increment
    fn apply_byte_edit_internal(
        &mut self,
        start_byte: usize,
        end_byte: usize,
        new_text: &str,
        edit_snapshot: &mut Snapshot,
    ) -> Result<TextDocumentContentChangeEvent> {
        // Get LSP range from deletion (if any)
        let lsp_range = if start_byte < end_byte {
            // Apply deletion and get the LSP range
            if let Some(deletion_result) =
                self.snapshot
                    .apply_deletion(edit_snapshot, start_byte, end_byte)
            {
                deletion_result.lsp_range
            } else {
                // No actual deletion occurred, use original positions
                let start_pos = self.byte_to_lsp_position(start_byte);
                let end_pos = self.byte_to_lsp_position(end_byte);
                Range::new(start_pos, end_pos)
            }
        } else {
            // Pure insertion - use the insertion point
            let pos = self.byte_to_lsp_position(start_byte);
            Range::new(pos, pos)
        };

        // Apply insertion if needed
        if !new_text.is_empty() {
            self.snapshot
                .apply_insertion(edit_snapshot, start_byte, new_text);
        }

        // Re-parse after edit
        self.reparse()?;

        Ok(TextDocumentContentChangeEvent {
            range: Some(lsp_range),
            range_length: None,
            text: new_text.to_string(),
        })
    }

    /// Apply an edit using byte offsets directly
    pub fn apply_byte_edit(
        &mut self,
        start_byte: usize,
        end_byte: usize,
        new_text: String,
        mut snapshot: Snapshot,
    ) -> Result<TextDocumentContentChangeEvent> {
        let result =
            self.apply_byte_edit_internal(start_byte, end_byte, &new_text, &mut snapshot)?;
        self.increment_version();
        Ok(result)
    }

    pub fn apply_text_edits(
        &mut self,
        edits: &[TextEdit],
        mut snapshot: Snapshot,
    ) -> Result<Vec<TextDocumentContentChangeEvent>> {
        let mut changes = Vec::new();

        for edit in edits.iter().rev() {
            let start_byte = Self::lsp_position_to_byte_with_rope(edit.range.start, &snapshot.rope);
            let end_byte = Self::lsp_position_to_byte_with_rope(edit.range.end, &snapshot.rope);

            changes.push(self.apply_byte_edit_internal(
                start_byte,
                end_byte,
                &edit.new_text,
                &mut snapshot,
            )?);
        }

        self.increment_version();
        changes.reverse(); // Reverse to restore original order
        Ok(changes)
    }

    /// Apply a single TextEdit from a snapshot
    pub fn apply_text_edit(
        &mut self,
        edit: TextEdit,
        mut snapshot: Snapshot,
    ) -> Result<TextDocumentContentChangeEvent> {
        let start_byte = Self::lsp_position_to_byte_with_rope(edit.range.start, &snapshot.rope);
        let end_byte = Self::lsp_position_to_byte_with_rope(edit.range.end, &snapshot.rope);

        let change =
            self.apply_byte_edit_internal(start_byte, end_byte, &edit.new_text, &mut snapshot)?;

        self.increment_version();
        Ok(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Range;

    #[test]
    fn test_basic_operations() {
        let mut editor = CrdtEditor::new("Hello, world!").unwrap();

        // Test insertion using apply_text_edit
        let insert_edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 7,
                },
                end: Position {
                    line: 0,
                    character: 7,
                },
            },
            new_text: "beautiful ".to_string(),
        };
        let snapshot = editor.fork();
        editor.apply_text_edit(insert_edit, snapshot).unwrap();
        assert_eq!(editor.get_text(), "Hello, beautiful world!");

        // Test deletion using apply_text_edit
        let delete_edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 7,
                },
                end: Position {
                    line: 0,
                    character: 17,
                },
            },
            new_text: String::new(),
        };
        let snapshot = editor.fork();
        editor.apply_text_edit(delete_edit, snapshot).unwrap();
        assert_eq!(editor.get_text(), "Hello, world!");

        // Test replacement using apply_text_edit
        let edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 7,
                },
                end: Position {
                    line: 0,
                    character: 12,
                },
            },
            new_text: "Rust".to_string(),
        };
        let snapshot = editor.fork();
        editor.apply_text_edit(edit, snapshot).unwrap();
        assert_eq!(editor.get_text(), "Hello, Rust!");
    }

    #[test]
    fn test_position_conversion() {
        let editor = CrdtEditor::new("line1\nline2\nline3").unwrap();

        // Test line/col to byte
        assert_eq!(editor.line_col_to_byte(0, 0), 0);
        assert_eq!(editor.line_col_to_byte(1, 0), 6);
        assert_eq!(editor.line_col_to_byte(2, 0), 12);
    }

    #[test]
    fn test_utf8_handling() {
        let mut editor = CrdtEditor::new("こんにちは").unwrap();

        // Japanese characters handling

        // Replace with emoji using apply_text_edit
        let edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 2,
                },
                end: Position {
                    line: 0,
                    character: 3,
                },
            },
            new_text: "🦀".to_string(),
        };
        let snapshot = editor.fork();
        editor.apply_text_edit(edit, snapshot).unwrap();
        assert_eq!(editor.get_text(), "こん🦀ちは");
    }

    #[test]
    fn test_apply_text_edit_returns_changes() {
        let mut editor = CrdtEditor::new("Hello, world!").unwrap();

        // Test insertion
        let insert_edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 7,
                },
                end: Position {
                    line: 0,
                    character: 7,
                },
            },
            new_text: "beautiful ".to_string(),
        };

        let snapshot = editor.fork();
        let changes = editor.apply_text_edit(insert_edit, snapshot).unwrap();
        assert_eq!(changes.text, "beautiful ");
        assert!(changes.range.is_some());
        assert_eq!(editor.get_text(), "Hello, beautiful world!");
    }

    #[test]
    fn test_apply_replacement_returns_changes() {
        let mut editor = CrdtEditor::new("Hello, world!").unwrap();

        // Test replacement (delete + insert)
        let replace_edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 7,
                },
                end: Position {
                    line: 0,
                    character: 12,
                },
            },
            new_text: "Rust".to_string(),
        };

        let snapshot = editor.fork();
        let changes = editor.apply_text_edit(replace_edit, snapshot).unwrap();

        // Replacement should produce 1 change event
        assert_eq!(changes.text, "Rust");
        assert_eq!(editor.get_text(), "Hello, Rust!");
    }
}
