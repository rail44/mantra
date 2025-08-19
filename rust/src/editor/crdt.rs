use anyhow::Result;
use cola::{Deletion, Insertion, Replica, ReplicaId};

/// CRDT-based collaborative text editor using cola
pub struct CrdtEditor {
    /// The cola replica for this editor instance
    replica: Replica,
    /// Replica ID for this editor instance
    replica_id: ReplicaId,
    /// The actual text content (cola tracks structure, we track content)
    text: String,
}

impl CrdtEditor {
    /// Create a new CRDT editor with a unique replica ID
    pub fn new(replica_id: ReplicaId, initial_text: &str) -> Self {
        let replica = Replica::new(replica_id, initial_text.len());

        Self {
            replica,
            replica_id,
            text: initial_text.to_string(),
        }
    }

    /// Create an editor from existing content
    pub fn from_text(text: &str) -> Self {
        // Generate a new replica ID based on timestamp and random component
        let replica_id = Self::generate_replica_id();
        Self::new(replica_id, text)
    }

    /// Generate a unique replica ID
    fn generate_replica_id() -> ReplicaId {
        // Use timestamp + random value for uniqueness
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let random = rand::random::<u32>() as u64;
        (timestamp ^ (random << 32)) as ReplicaId
    }

    /// Get the current text content
    pub fn get_text(&self) -> &str {
        &self.text
    }

    /// Insert text at a specific position
    pub fn insert(&mut self, position: usize, text: &str) -> Result<Insertion> {
        // Create the insertion operation
        let insertion = self.replica.inserted(position, text.len());

        // Apply to our replica
        let _ = self.replica.integrate_insertion(&insertion);

        // Update our text
        self.text.insert_str(position, text);

        Ok(insertion)
    }

    /// Delete text in a range
    pub fn delete(&mut self, start: usize, end: usize) -> Result<Deletion> {
        // Create the deletion operation
        let deletion = self.replica.deleted(start..end);

        // Apply to our replica
        let _ = self.replica.integrate_deletion(&deletion);

        // Update our text
        self.text.replace_range(start..end, "");

        Ok(deletion)
    }

    /// Replace text in a range
    pub fn replace(
        &mut self,
        start: usize,
        end: usize,
        text: &str,
    ) -> Result<(Option<Deletion>, Insertion)> {
        let deletion = if start < end {
            Some(self.delete(start, end)?)
        } else {
            None
        };
        let insertion = self.insert(start, text)?;
        Ok((deletion, insertion))
    }

    /// Apply an insertion from another replica
    pub fn apply_remote_insertion(&mut self, insertion: &Insertion, text: &str) -> Result<()> {
        // Integrate the insertion into our CRDT
        let _ = self.replica.integrate_insertion(insertion);

        // We need to map the CRDT position to our text position
        // For now, we'll use a simple approach
        // In a real implementation, we'd need proper position mapping
        let position = self.crdt_to_text_position(insertion);
        self.text.insert_str(position, text);

        Ok(())
    }

    /// Apply a deletion from another replica
    pub fn apply_remote_deletion(&mut self, deletion: &Deletion) -> Result<()> {
        // Integrate the deletion into our CRDT
        let _ = self.replica.integrate_deletion(deletion);

        // Map CRDT deletion to text positions and apply
        let (start, end) = self.crdt_deletion_to_text_range(deletion);
        if start < end && end <= self.text.len() {
            self.text.replace_range(start..end, "");
        }

        Ok(())
    }

    /// Map CRDT position to text position
    fn crdt_to_text_position(&self, _insertion: &Insertion) -> usize {
        // This is a simplified implementation
        // In reality, we'd need to track the mapping between CRDT and text positions
        0 // Placeholder
    }

    /// Map CRDT deletion to text range
    fn crdt_deletion_to_text_range(&self, _deletion: &Deletion) -> (usize, usize) {
        // This is a simplified implementation
        // In reality, we'd need to track the mapping between CRDT and text positions
        (0, 0) // Placeholder
    }

    /// Get the replica ID
    pub fn replica_id(&self) -> ReplicaId {
        self.replica_id
    }

    /// Get the current length of the document
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Check if the document is empty
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Convert byte position to line/column
    pub fn byte_to_line_col(&self, byte_pos: usize) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;
        let mut current_byte = 0;

        for ch in self.text.chars() {
            if current_byte >= byte_pos {
                break;
            }

            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += ch.len_utf8();
            }

            current_byte += ch.len_utf8();
        }

        (line, col)
    }

    /// Convert line/column to byte position
    pub fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        let mut current_line = 0;
        let mut byte_offset = 0;
        let mut line_start_byte = 0;

        for ch in self.text.chars() {
            if current_line == line && (byte_offset - line_start_byte) >= col {
                return byte_offset;
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

        line_start_byte + col
    }

    /// Fork this replica to create a new one
    pub fn fork(&self) -> Self {
        let new_replica_id = Self::generate_replica_id();
        let new_replica = self.replica.fork(new_replica_id);

        Self {
            replica: new_replica,
            replica_id: new_replica_id,
            text: self.text.clone(),
        }
    }

    /// Get a snapshot of the current state for synchronization
    pub fn get_replica(&self) -> &Replica {
        &self.replica
    }
}

/// Operations that can be applied to the CRDT
#[derive(Debug, Clone)]
pub enum CrdtOperation {
    Insert { insertion: Insertion, text: String },
    Delete(Deletion),
}

impl CrdtOperation {
    /// Apply this operation to an editor
    pub fn apply_to(&self, editor: &mut CrdtEditor) -> Result<()> {
        match self {
            CrdtOperation::Insert { insertion, text } => {
                editor.apply_remote_insertion(insertion, text)
            }
            CrdtOperation::Delete(deletion) => editor.apply_remote_deletion(deletion),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut editor = CrdtEditor::from_text("Hello, world!");

        // Test insertion
        editor.insert(7, "beautiful ").unwrap();
        assert_eq!(editor.get_text(), "Hello, beautiful world!");

        // Test deletion
        editor.delete(7, 17).unwrap();
        assert_eq!(editor.get_text(), "Hello, world!");

        // Test replacement
        editor.replace(7, 12, "Rust").unwrap();
        assert_eq!(editor.get_text(), "Hello, Rust!");
    }

    #[test]
    fn test_position_conversion() {
        let editor = CrdtEditor::from_text("line1\nline2\nline3");

        // Test byte to line/col
        assert_eq!(editor.byte_to_line_col(0), (0, 0));
        assert_eq!(editor.byte_to_line_col(6), (1, 0));
        assert_eq!(editor.byte_to_line_col(12), (2, 0));

        // Test line/col to byte
        assert_eq!(editor.line_col_to_byte(0, 0), 0);
        assert_eq!(editor.line_col_to_byte(1, 0), 6);
        assert_eq!(editor.line_col_to_byte(2, 0), 12);
    }

    #[test]
    fn test_empty_document() {
        let editor = CrdtEditor::from_text("");
        assert!(editor.is_empty());
        assert_eq!(editor.len(), 0);
        assert_eq!(editor.get_text(), "");
    }

    #[test]
    fn test_utf8_handling() {
        let mut editor = CrdtEditor::from_text("こんにちは");

        // Japanese characters handling
        assert_eq!(editor.byte_to_line_col(0), (0, 0));
        assert_eq!(editor.byte_to_line_col(3), (0, 3));
        assert_eq!(editor.byte_to_line_col(6), (0, 6));

        // Replace with emoji
        editor.replace(6, 9, "🦀").unwrap();
        assert_eq!(editor.get_text(), "こん🦀ちは");
    }

    #[test]
    fn test_fork() {
        let editor1 = CrdtEditor::from_text("Hello");
        let editor2 = editor1.fork();

        assert_ne!(editor1.replica_id(), editor2.replica_id());
        assert_eq!(editor1.get_text(), editor2.get_text());
    }
}
