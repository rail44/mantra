use cola::{Replica, ReplicaId};
use crop::Rope;
use lsp_types::{Position, Range, TextDocumentContentChangeEvent, TextEdit};

/// CRDT-based collaborative text editor using cola and crop
#[derive(Debug, Clone)]
pub struct CrdtEditor {
    /// The cola replica for this editor instance
    replica: Replica,
    /// The text content using crop's Rope for efficient editing
    rope: Rope,
    /// Document version for LSP synchronization
    version: i32,
}

impl CrdtEditor {
    /// Create a new CRDT editor with a unique replica ID
    pub fn new(initial_text: &str) -> Self {
        let id = Self::generate_replica_id();
        let replica = Replica::new(id, initial_text.len());
        let rope = Rope::from(initial_text);

        Self {
            replica,
            rope,
            version: 0,
        }
    }

    /// Generate a unique replica ID using fast random number generation
    fn generate_replica_id() -> ReplicaId {
        // Use fastrand for fast, non-cryptographic random number generation
        fastrand::u64(..) as ReplicaId
    }

    /// Get the current text content
    pub fn get_text(&self) -> String {
        self.rope.to_string()
    }

    /// Apply deletion and return the deleted range (start, end)
    fn apply_deletion(
        &mut self,
        snapshot: &mut Self,
        start: usize,
        end: usize,
    ) -> Option<(usize, usize)> {
        let deletion = snapshot.replica.deleted(start..end);
        let ranges = self.replica.integrate_deletion(&deletion);

        if ranges.is_empty() {
            return None;
        }

        let first_start = ranges.first().unwrap().start;
        let last_end = ranges.last().unwrap().end;

        // Apply deletions to rope in reverse order
        for range in ranges.iter().rev() {
            self.rope.delete(range.clone());
        }

        Some((first_start, last_end))
    }

    /// Apply insertion and return the actual insertion position
    fn apply_insertion(
        &mut self,
        snapshot: &mut Self,
        position: usize,
        text: &str,
    ) -> Option<usize> {
        let insertion = snapshot.replica.inserted(position, text.len());

        if let Some(actual_pos) = self.replica.integrate_insertion(&insertion) {
            self.rope.insert(actual_pos, text);
            Some(actual_pos)
        } else {
            None
        }
    }

    fn lsp_position_to_byte_with_rope(position: Position, rope: &Rope) -> usize {
        let line_start_byte = rope.byte_of_line(position.line as usize);
        let line_start_utf16 = rope.utf16_code_unit_of_byte(line_start_byte);

        // Calculate the target UTF-16 position
        let target_utf16 = line_start_utf16 + position.character as usize;

        // Convert to byte position
        rope.byte_of_utf16_code_unit(target_utf16)
    }

    /// Convert byte position to LSP position using provided rope
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

    /// Convert line/column to byte position
    pub fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        let line_start = self.rope.byte_of_line(line);
        line_start + col.min(self.rope.line(line).byte_len())
    }

    /// Get the current document version
    pub fn get_version(&self) -> i32 {
        self.version
    }

    /// Increment the document version and return the new version
    fn increment_version(&mut self) -> i32 {
        self.version += 1;
        self.version
    }

    /// Create a snapshot of the current state
    pub fn fork(&self) -> Self {
        // Fork the replica with a new random ID for this snapshot
        let snapshot_replica_id = Self::generate_replica_id();
        let replica = self.replica.fork(snapshot_replica_id);

        Self {
            version: self.version,
            replica,
            rope: self.rope.clone(),
        }
    }

    /// Apply a single TextEdit and return the change event
    fn apply_single_edit(
        &mut self,
        edit: &TextEdit,
        snapshot: &mut Self,
    ) -> TextDocumentContentChangeEvent {
        let start_byte = Self::lsp_position_to_byte_with_rope(edit.range.start, &snapshot.rope);
        let end_byte = Self::lsp_position_to_byte_with_rope(edit.range.end, &snapshot.rope);

        // Convert byte positions to LSP positions BEFORE any changes
        let start_pos = self.byte_to_lsp_position(start_byte);
        let end_pos = self.byte_to_lsp_position(end_byte);

        // Apply deletion if needed
        if start_byte < end_byte {
            self.apply_deletion(snapshot, start_byte, end_byte);
        }

        // Apply insertion if needed
        if !edit.new_text.is_empty() {
            self.apply_insertion(snapshot, start_byte, &edit.new_text);
        }

        // Return the change event
        TextDocumentContentChangeEvent {
            range: Some(Range::new(start_pos, end_pos)),
            range_length: None,
            text: edit.new_text.clone(),
        }
    }

    pub fn apply_text_edits(
        &mut self,
        edits: &[TextEdit],
        mut snapshot: Self,
    ) -> Vec<TextDocumentContentChangeEvent> {
        let mut changes = Vec::new();

        for edit in edits.iter().rev() {
            changes.push(self.apply_single_edit(edit, &mut snapshot));
        }

        self.increment_version();
        changes.reverse(); // Reverse to restore original order
        changes
    }

    /// Apply a single TextEdit from a snapshot
    pub fn apply_text_edit(
        &mut self,
        edit: TextEdit,
        snapshot: Self,
    ) -> Vec<TextDocumentContentChangeEvent> {
        self.apply_text_edits(&[edit], snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Range;

    #[test]
    fn test_basic_operations() {
        let mut editor = CrdtEditor::new("Hello, world!");

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
        editor.apply_text_edit(insert_edit, snapshot);
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
        editor.apply_text_edit(delete_edit, snapshot);
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
        editor.apply_text_edit(edit, snapshot);
        assert_eq!(editor.get_text(), "Hello, Rust!");
    }

    #[test]
    fn test_position_conversion() {
        let editor = CrdtEditor::new("line1\nline2\nline3");

        // Test line/col to byte
        assert_eq!(editor.line_col_to_byte(0, 0), 0);
        assert_eq!(editor.line_col_to_byte(1, 0), 6);
        assert_eq!(editor.line_col_to_byte(2, 0), 12);
    }

    #[test]
    fn test_utf8_handling() {
        let mut editor = CrdtEditor::new("こんにちは");

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
        editor.apply_text_edit(edit, snapshot);
        assert_eq!(editor.get_text(), "こん🦀ちは");
    }

    #[test]
    fn test_apply_text_edit_returns_changes() {
        let mut editor = CrdtEditor::new("Hello, world!");

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
        let changes = editor.apply_text_edit(insert_edit, snapshot);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].text, "beautiful ");
        assert!(changes[0].range.is_some());
        assert_eq!(editor.get_text(), "Hello, beautiful world!");
    }

    #[test]
    fn test_apply_replacement_returns_changes() {
        let mut editor = CrdtEditor::new("Hello, world!");

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
        let changes = editor.apply_text_edit(replace_edit, snapshot);

        // Replacement should produce 1 change event
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].text, "Rust");
        assert_eq!(editor.get_text(), "Hello, Rust!");
    }
}
