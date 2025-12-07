use anyhow::Result;
use cola::{Anchor, Deletion, Insertion, Replica, ReplicaId};
use crop::Rope;
use lsp_types::{Position, Range, TextDocumentContentChangeEvent, TextEdit};
use std::ops::Range as StdRange;
use tree_sitter::Tree;

use crate::parser::GoParser;

/// Convert LSP position to byte position in rope
///
/// LSP positions use UTF-16 code units for the character offset,
/// so we need to convert through UTF-16 to get the correct byte position.
pub fn lsp_position_to_byte(position: Position, rope: &Rope) -> usize {
    let line_start_byte = rope.byte_of_line(position.line as usize);
    let line_start_utf16 = rope.utf16_code_unit_of_byte(line_start_byte);
    let target_utf16 = line_start_utf16 + position.character as usize;
    rope.byte_of_utf16_code_unit(target_utf16)
}

/// Result of a deletion operation
#[derive(Debug)]
pub struct DeletionResult {
    pub lsp_range: Range,
}

/// Edit operation that can be propagated to other `CrdtEditors`
#[derive(Debug)]
pub struct EditOperation {
    /// The cola Deletion (if any)
    pub deletion: Option<Deletion>,
    /// The cola Insertion (if any)
    pub insertion: Option<Insertion>,
    /// The inserted text (needed for integration)
    pub inserted_text: String,
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
            line: u32::try_from(line).unwrap(),
            character: u32::try_from(utf16_col).unwrap(),
        }
    }

    /// Apply deletion and return the deletion result with LSP positions
    pub fn apply_deletion(
        &mut self,
        edit_snapshot: &mut Snapshot,
        range: &StdRange<usize>,
    ) -> Option<DeletionResult> {
        let deletion = edit_snapshot.replica.deleted(range.clone());
        let ranges = self.replica.integrate_deletion(&deletion);

        if ranges.is_empty() {
            return None;
        }

        // Safe to unwrap because we checked ranges is not empty
        let first_range = ranges.first().expect("ranges should not be empty");
        let last_range = ranges.last().expect("ranges should not be empty");
        let byte_range = first_range.start..last_range.end;

        // Calculate LSP positions before deletion
        let start_pos = self.byte_to_lsp_position(byte_range.start);
        let end_pos = self.byte_to_lsp_position(byte_range.end);

        // Apply deletions to rope in reverse order
        for range in ranges.iter().rev() {
            self.rope.delete(range.clone());
        }

        Some(DeletionResult {
            lsp_range: Range::new(start_pos, end_pos),
        })
    }

    /// Apply insertion
    pub fn apply_insertion(&mut self, edit_snapshot: &mut Snapshot, position: usize, text: &str) {
        let insertion = edit_snapshot.replica.inserted(position, text.len());

        if let Some(actual_pos) = self.replica.integrate_insertion(&insertion) {
            self.rope.insert(actual_pos, text);
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
                .map_err(|e| anyhow::anyhow!("Failed to parse: {e}"))?,
        );
        Ok(())
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
            line: u32::try_from(line).unwrap(),
            character: u32::try_from(utf16_col).unwrap(),
        }
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

    /// Resolve an anchor to its current byte position
    ///
    /// Returns None if the anchor cannot be resolved (e.g., the anchor
    /// references edits that this replica doesn't have).
    pub fn resolve_anchor(&self, anchor: Anchor) -> Option<usize> {
        self.snapshot.replica.resolve_anchor(anchor)
    }

    /// Internal byte edit without version increment
    fn apply_byte_edit_internal(
        &mut self,
        byte_range: &StdRange<usize>,
        new_text: &str,
        edit_snapshot: &mut Snapshot,
    ) -> Result<TextDocumentContentChangeEvent> {
        // Get LSP range from deletion (if any)
        let lsp_range = if byte_range.start < byte_range.end {
            // Apply deletion and get the LSP range
            if let Some(deletion_result) = self.snapshot.apply_deletion(edit_snapshot, byte_range) {
                deletion_result.lsp_range
            } else {
                // No actual deletion occurred, use original positions
                let start_pos = self.byte_to_lsp_position(byte_range.start);
                let end_pos = self.byte_to_lsp_position(byte_range.end);
                Range::new(start_pos, end_pos)
            }
        } else {
            // Pure insertion - use the insertion point
            let pos = self.byte_to_lsp_position(byte_range.start);
            Range::new(pos, pos)
        };

        // Apply insertion if needed
        if !new_text.is_empty() {
            self.snapshot
                .apply_insertion(edit_snapshot, byte_range.start, new_text);
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
        byte_range: &StdRange<usize>,
        new_text: &str,
        mut snapshot: Snapshot,
    ) -> Result<TextDocumentContentChangeEvent> {
        let result = self.apply_byte_edit_internal(byte_range, new_text, &mut snapshot)?;
        self.increment_version();
        Ok(result)
    }

    /// Apply an edit and return the `EditOperation` for propagation to other editors
    ///
    /// Unlike `apply_byte_edit`, this method creates Insertion/Deletion in self's replica
    /// and returns them so they can be integrated into other editors.
    pub fn apply_byte_edit_with_ops(
        &mut self,
        byte_range: &StdRange<usize>,
        new_text: &str,
    ) -> Result<EditOperation> {
        // Create deletion if needed
        let deletion = if byte_range.start < byte_range.end {
            let del = self.snapshot.replica.deleted(byte_range.clone());
            // Apply deletion to rope
            // For local edits, we directly delete the range (no coordinate transformation needed)
            self.snapshot.rope.delete(byte_range.clone());
            Some(del)
        } else {
            None
        };

        // Create insertion if needed
        let insertion = if new_text.is_empty() {
            None
        } else {
            let ins = self
                .snapshot
                .replica
                .inserted(byte_range.start, new_text.len());
            // Apply insertion to rope
            self.snapshot.rope.insert(byte_range.start, new_text);
            Some(ins)
        };

        // Re-parse after edit
        self.reparse()?;
        self.increment_version();

        Ok(EditOperation {
            deletion,
            insertion,
            inserted_text: new_text.to_string(),
        })
    }

    /// Integrate an `EditOperation` from another editor
    ///
    /// This applies the Insertion/Deletion from another editor, with cola
    /// handling coordinate transformation automatically.
    pub fn integrate_ops(&mut self, ops: &EditOperation) -> Result<()> {
        // Integrate deletion first (if any)
        if let Some(ref deletion) = ops.deletion {
            let ranges = self.snapshot.replica.integrate_deletion(deletion);
            // Apply deletions to rope in reverse order
            for range in ranges.iter().rev() {
                self.snapshot.rope.delete(range.clone());
            }
        }

        // Integrate insertion (if any)
        if let Some(ref insertion) = ops.insertion {
            if let Some(pos) = self.snapshot.replica.integrate_insertion(insertion) {
                self.snapshot.rope.insert(pos, &ops.inserted_text);
            }
        }

        // Re-parse after edit
        self.reparse()?;
        self.increment_version();

        Ok(())
    }

    /// Create a forked editor that shares ancestry with this editor
    ///
    /// The forked editor can receive `EditOperations` from this editor via `integrate_ops`,
    /// and cola will correctly handle coordinate transformation.
    pub fn fork_editor(&self) -> Result<Self> {
        let snapshot = Snapshot {
            replica: self.snapshot.replica.fork(Self::generate_replica_id()),
            rope: self.snapshot.rope.clone(),
            version: self.snapshot.version,
        };

        let parser = GoParser::new()?;
        let mut editor = Self {
            snapshot,
            parser,
            tree: None,
        };
        editor.reparse()?;
        Ok(editor)
    }

    pub fn apply_text_edits(
        &mut self,
        edits: &[TextEdit],
        mut snapshot: Snapshot,
    ) -> Result<Vec<TextDocumentContentChangeEvent>> {
        let mut changes = Vec::new();

        for edit in edits.iter().rev() {
            let start_byte = lsp_position_to_byte(edit.range.start, &snapshot.rope);
            let end_byte = lsp_position_to_byte(edit.range.end, &snapshot.rope);

            changes.push(self.apply_byte_edit_internal(
                &(start_byte..end_byte),
                &edit.new_text,
                &mut snapshot,
            )?);
        }

        self.increment_version();
        changes.reverse(); // Reverse to restore original order
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut editor = CrdtEditor::new("Hello, world!").unwrap();

        // Test insertion using apply_byte_edit
        let snapshot = editor.fork();
        editor
            .apply_byte_edit(&(7..7), "beautiful ", snapshot)
            .unwrap();
        assert_eq!(editor.get_text(), "Hello, beautiful world!");

        // Test deletion using apply_byte_edit
        let snapshot = editor.fork();
        editor.apply_byte_edit(&(7..17), "", snapshot).unwrap();
        assert_eq!(editor.get_text(), "Hello, world!");

        // Test replacement using apply_byte_edit
        let snapshot = editor.fork();
        editor.apply_byte_edit(&(7..12), "Rust", snapshot).unwrap();
        assert_eq!(editor.get_text(), "Hello, Rust!");
    }

    #[test]
    fn test_fork_editor_and_integrate_ops() {
        // Create an editor and fork it
        let mut editor_sync = CrdtEditor::new("func Foo() {}").unwrap();
        let mut generation_editor = editor_sync.fork_editor().unwrap();

        // Both should have the same initial content
        assert_eq!(editor_sync.get_text(), "func Foo() {}");
        assert_eq!(generation_editor.get_text(), "func Foo() {}");

        // Apply an edit to editor_sync and get the ops
        let ops = editor_sync
            .apply_byte_edit_with_ops(&(5..8), "Bar")
            .unwrap();
        assert_eq!(editor_sync.get_text(), "func Bar() {}");

        // Integrate the ops into generation_editor
        generation_editor.integrate_ops(&ops).unwrap();
        assert_eq!(generation_editor.get_text(), "func Bar() {}");
    }

    #[test]
    fn test_anchor_across_forked_replicas() {
        use cola::AnchorBias;

        // Create an editor and fork it
        let mut editor_sync = CrdtEditor::new("func Foo() {}").unwrap();
        let mut generation_editor = editor_sync.fork_editor().unwrap();

        // Create an anchor at position 5 (start of "Foo") in editor_sync
        let foo_anchor = editor_sync
            .snapshot
            .replica
            .create_anchor(5, AnchorBias::Left);

        // The anchor should resolve in both editors initially
        assert_eq!(
            editor_sync.snapshot.replica.resolve_anchor(foo_anchor),
            Some(5)
        );
        assert_eq!(
            generation_editor
                .snapshot
                .replica
                .resolve_anchor(foo_anchor),
            Some(5)
        );

        // Apply an edit to editor_sync (insert "X" at position 0)
        let ops = editor_sync.apply_byte_edit_with_ops(&(0..0), "X").unwrap();
        generation_editor.integrate_ops(&ops).unwrap();

        // The anchor should now resolve to position 6 in both editors
        assert_eq!(
            editor_sync.snapshot.replica.resolve_anchor(foo_anchor),
            Some(6)
        );
        assert_eq!(
            generation_editor
                .snapshot
                .replica
                .resolve_anchor(foo_anchor),
            Some(6)
        );

        // Apply generated code to generation_editor only
        let _gen_ops = generation_editor
            .apply_byte_edit_with_ops(&(12..14), "{ return 42 }")
            .unwrap();

        // The anchor should still resolve in both editors
        // In editor_sync: still at position 6
        // In generation_editor: still at position 6 (before the generated code change)
        assert_eq!(
            editor_sync.snapshot.replica.resolve_anchor(foo_anchor),
            Some(6)
        );
        // generation_editor has the edit, so anchor should still resolve
        assert_eq!(
            generation_editor
                .snapshot
                .replica
                .resolve_anchor(foo_anchor),
            Some(6)
        );
    }

    #[test]
    fn test_fork_with_divergent_state() {
        // Simulate the scenario where generation_editor has generated code
        // that editor_sync doesn't have
        // "func Foo() {}" - positions: func=0-4, Foo=5-8, ()=8-10, space=10, {}=11-13
        let mut editor_sync = CrdtEditor::new("func Foo() {}").unwrap();
        let mut generation_editor = editor_sync.fork_editor().unwrap();

        // Apply generated code to generation_editor only
        // Replace {} (positions 11-13) with { return 42 }
        let _gen_ops = generation_editor
            .apply_byte_edit_with_ops(&(11..13), "{ return 42 }")
            .unwrap();
        assert_eq!(generation_editor.get_text(), "func Foo() { return 42 }");
        assert_eq!(editor_sync.get_text(), "func Foo() {}");

        // Now user edits Foo to Bar in editor_sync
        let user_ops = editor_sync
            .apply_byte_edit_with_ops(&(5..8), "Bar")
            .unwrap();
        assert_eq!(editor_sync.get_text(), "func Bar() {}");

        // Integrate user edit into generation_editor
        // Cola should transform coordinates to account for the generated content
        generation_editor.integrate_ops(&user_ops).unwrap();

        // The function name should be changed, and generated content should be preserved
        assert_eq!(generation_editor.get_text(), "func Bar() { return 42 }");
    }
}
