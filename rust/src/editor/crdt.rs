use cola::{Replica, ReplicaId};
use crop::Rope;
use lsp_types::{Position, TextEdit};

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

    /// Apply insertion to rope at given position with text
    fn apply_insertion_to_rope(&mut self, position: usize, text: &str) {
        self.rope.insert(position, text);
    }

    /// Apply deletions to rope for given ranges
    fn apply_deletions_to_rope(&mut self, ranges: &[std::ops::Range<usize>]) {
        // Apply deletions in reverse order to maintain position validity
        for range in ranges.iter().rev() {
            self.rope.delete(range.clone());
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

    pub fn apply_text_edits(&mut self, edits: &[TextEdit], mut snapshot: Self) {
        // Apply each edit in sequence
        for edit in edits.iter().rev() {
            // Convert LSP positions to byte positions using snapshot's rope
            let start_byte = Self::lsp_position_to_byte_with_rope(edit.range.start, &snapshot.rope);
            let end_byte = Self::lsp_position_to_byte_with_rope(edit.range.end, &snapshot.rope);

            // Positions calculated from snapshot

            // Handle deletion if needed
            if start_byte < end_byte {
                let deletion = snapshot.replica.deleted(start_byte..end_byte);
                let ranges = self.replica.integrate_deletion(&deletion);
                self.apply_deletions_to_rope(&ranges);
            }

            // Handle insertion if needed
            if !edit.new_text.is_empty() {
                let insertion = snapshot.replica.inserted(start_byte, edit.new_text.len());
                if let Some(position) = self.replica.integrate_insertion(&insertion) {
                    self.apply_insertion_to_rope(position, &edit.new_text);
                }
            }
        }

        self.increment_version();
    }

    /// Apply a TextEdit from a snapshot and return a new snapshot
    pub fn apply_text_edit(&mut self, edit: TextEdit, snapshot: Self) {
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
        let snapshot = editor.fork();
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
        editor.apply_text_edit(insert_edit, snapshot);
        assert_eq!(editor.get_text(), "Hello, beautiful world!");

        // Test deletion using apply_text_edit
        let snapshot = editor.fork();
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
        editor.apply_text_edit(delete_edit, snapshot);
        assert_eq!(editor.get_text(), "Hello, world!");

        // Test replacement using apply_text_edit
        let snapshot = editor.fork();
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
        let snapshot = editor.fork();
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
        editor.apply_text_edit(edit, snapshot);
        assert_eq!(editor.get_text(), "こん🦀ちは");
    }
}
