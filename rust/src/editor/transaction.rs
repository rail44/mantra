use anyhow::Result;
use cola::{Deletion, Insertion, Replica};
use std::collections::VecDeque;

use super::crdt::{CrdtEditor, CrdtOperation};

/// Maximum number of snapshots to keep in history
const MAX_HISTORY_SIZE: usize = 100;

/// A snapshot of the editor state at a specific version
#[derive(Clone, Debug)]
pub struct EditorSnapshot {
    /// Version number of this snapshot
    pub version: u64,
    /// The replica state at this version
    pub replica: Replica,
    /// The text content at this version
    pub text: String,
    /// Description of what changed in this version
    pub description: Option<String>,
}

/// Transaction represents a series of operations that can be committed or rolled back
#[derive(Debug)]
pub struct Transaction {
    /// Unique ID for this transaction
    pub id: String,
    /// Operations performed in this transaction
    pub operations: Vec<CrdtOperation>,
    /// The starting version of this transaction
    pub start_version: u64,
    /// Description of the transaction (e.g., "LLM generation for function X")
    pub description: String,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(id: String, start_version: u64, description: String) -> Self {
        Self {
            id,
            operations: Vec::new(),
            start_version,
            description,
        }
    }

    /// Add an operation to this transaction
    pub fn add_operation(&mut self, operation: CrdtOperation) {
        self.operations.push(operation);
    }
}

/// CRDT editor with transaction and version management
pub struct TransactionalCrdtEditor {
    /// The underlying CRDT editor
    editor: CrdtEditor,
    /// Current version number
    version: u64,
    /// History of snapshots for rollback
    history: VecDeque<EditorSnapshot>,
    /// Active transactions
    active_transactions: Vec<Transaction>,
}

impl TransactionalCrdtEditor {
    /// Create a new transactional editor from text
    pub fn new(text: &str) -> Self {
        let editor = CrdtEditor::from_text(text);
        let initial_snapshot = EditorSnapshot {
            version: 0,
            replica: editor.get_replica().clone(),
            text: text.to_string(),
            description: Some("Initial state".to_string()),
        };

        let mut history = VecDeque::new();
        history.push_back(initial_snapshot);

        Self {
            editor,
            version: 0,
            history,
            active_transactions: Vec::new(),
        }
    }

    /// Get the current text
    pub fn get_text(&self) -> &str {
        self.editor.get_text()
    }

    /// Get the current version
    pub fn get_version(&self) -> u64 {
        self.version
    }

    /// Begin a new transaction
    pub fn begin_transaction(&mut self, id: String, description: String) -> Result<()> {
        // Check if transaction with same ID already exists
        if self.active_transactions.iter().any(|t| t.id == id) {
            return Err(anyhow::anyhow!("Transaction {} already exists", id));
        }

        let transaction = Transaction::new(id, self.version, description);
        self.active_transactions.push(transaction);
        Ok(())
    }

    /// Perform an insert operation within a transaction
    pub fn insert_in_transaction(
        &mut self,
        transaction_id: &str,
        position: usize,
        text: &str,
    ) -> Result<Insertion> {
        let insertion = self.editor.insert(position, text)?;

        // Add to transaction if it exists
        if let Some(transaction) = self
            .active_transactions
            .iter_mut()
            .find(|t| t.id == transaction_id)
        {
            transaction.add_operation(CrdtOperation::Insert {
                insertion: insertion.clone(),
                text: text.to_string(),
            });
        }

        Ok(insertion)
    }

    /// Perform a delete operation within a transaction
    pub fn delete_in_transaction(
        &mut self,
        transaction_id: &str,
        start: usize,
        end: usize,
    ) -> Result<Deletion> {
        let deletion = self.editor.delete(start, end)?;

        // Add to transaction if it exists
        if let Some(transaction) = self
            .active_transactions
            .iter_mut()
            .find(|t| t.id == transaction_id)
        {
            transaction.add_operation(CrdtOperation::Delete(deletion.clone()));
        }

        Ok(deletion)
    }

    /// Commit a transaction, creating a new version
    pub fn commit_transaction(&mut self, transaction_id: &str) -> Result<u64> {
        // Find and remove the transaction
        let transaction_index = self
            .active_transactions
            .iter()
            .position(|t| t.id == transaction_id)
            .ok_or_else(|| anyhow::anyhow!("Transaction {} not found", transaction_id))?;

        let transaction = self.active_transactions.remove(transaction_index);

        // Increment version
        self.version += 1;

        // Create a snapshot
        let snapshot = EditorSnapshot {
            version: self.version,
            replica: self.editor.get_replica().clone(),
            text: self.editor.get_text().to_string(),
            description: Some(transaction.description),
        };

        self.history.push_back(snapshot);

        // Limit history size
        if self.history.len() > MAX_HISTORY_SIZE {
            self.history.pop_front();
        }

        Ok(self.version)
    }

    /// Rollback a transaction, reverting all its operations
    pub fn rollback_transaction(&mut self, transaction_id: &str) -> Result<()> {
        // Find the transaction
        let transaction_index = self
            .active_transactions
            .iter()
            .position(|t| t.id == transaction_id)
            .ok_or_else(|| anyhow::anyhow!("Transaction {} not found", transaction_id))?;

        let transaction = self.active_transactions.remove(transaction_index);

        // Find the snapshot at the transaction's start version
        let snapshot = self
            .history
            .iter()
            .find(|s| s.version == transaction.start_version)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Snapshot for version {} not found",
                    transaction.start_version
                )
            })?
            .clone();

        // Restore the editor state by creating a new editor with the snapshot text
        self.editor = CrdtEditor::from_text(&snapshot.text);

        Ok(())
    }

    /// Rollback to a specific version
    pub fn rollback_to_version(&mut self, version: u64) -> Result<()> {
        let snapshot = self
            .history
            .iter()
            .find(|s| s.version == version)
            .ok_or_else(|| anyhow::anyhow!("Version {} not found in history", version))?
            .clone();

        // Restore the editor state by creating a new editor with the snapshot text
        self.editor = CrdtEditor::from_text(&snapshot.text);
        self.version = version;

        // Remove all transactions that started after this version
        self.active_transactions
            .retain(|t| t.start_version <= version);

        Ok(())
    }

    /// Get all active transactions
    pub fn get_active_transactions(&self) -> &[Transaction] {
        &self.active_transactions
    }

    /// Check if there are any active transactions
    pub fn has_active_transactions(&self) -> bool {
        !self.active_transactions.is_empty()
    }

    /// Get the history of snapshots
    pub fn get_history(&self) -> &VecDeque<EditorSnapshot> {
        &self.history
    }

    /// Create a fork for parallel editing
    pub fn fork(&self) -> Self {
        let forked_editor = self.editor.fork();
        let initial_snapshot = EditorSnapshot {
            version: self.version,
            replica: forked_editor.get_replica().clone(),
            text: forked_editor.get_text().to_string(),
            description: Some(format!("Forked from version {}", self.version)),
        };

        let mut history = VecDeque::new();
        history.push_back(initial_snapshot);

        Self {
            editor: forked_editor,
            version: self.version,
            history,
            active_transactions: Vec::new(),
        }
    }

    /// Detect conflicts between two versions
    pub fn detect_conflicts(&self, other: &Self) -> Vec<ConflictInfo> {
        let mut conflicts = Vec::new();

        // Simple conflict detection: if the texts differ at the same version
        if self.version == other.version && self.get_text() != other.get_text() {
            conflicts.push(ConflictInfo {
                version: self.version,
                description: "Text mismatch at same version".to_string(),
            });
        }

        conflicts
    }

    /// Get the underlying CRDT editor (for compatibility)
    pub fn editor(&self) -> &CrdtEditor {
        &self.editor
    }

    /// Get a mutable reference to the underlying CRDT editor
    pub fn editor_mut(&mut self) -> &mut CrdtEditor {
        &mut self.editor
    }

    /// Convert byte position to line/column
    pub fn byte_to_line_col(&self, byte_pos: usize) -> (usize, usize) {
        self.editor.byte_to_line_col(byte_pos)
    }

    /// Convert line/column to byte position
    pub fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        self.editor.line_col_to_byte(line, col)
    }

    /// Replace text in a range (without transaction)
    pub fn replace(&mut self, start: usize, end: usize, text: &str) -> Result<()> {
        self.editor.replace(start, end, text)?;
        Ok(())
    }
}

/// Information about a detected conflict
#[derive(Debug, Clone)]
pub struct ConflictInfo {
    pub version: u64,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_commit() {
        let mut editor = TransactionalCrdtEditor::new("Hello world");

        // Begin a transaction
        editor
            .begin_transaction("test".to_string(), "Test transaction".to_string())
            .unwrap();

        // Make changes within the transaction
        editor
            .insert_in_transaction("test", 5, " beautiful")
            .unwrap();

        // Commit the transaction
        let version = editor.commit_transaction("test").unwrap();
        assert_eq!(version, 1);
        assert_eq!(editor.get_text(), "Hello beautiful world");
    }

    #[test]
    fn test_transaction_rollback() {
        let mut editor = TransactionalCrdtEditor::new("Hello world");

        // Begin a transaction
        editor
            .begin_transaction("test".to_string(), "Test transaction".to_string())
            .unwrap();

        // Make changes within the transaction
        editor
            .insert_in_transaction("test", 5, " beautiful")
            .unwrap();
        assert_eq!(editor.get_text(), "Hello beautiful world");

        // Rollback the transaction
        editor.rollback_transaction("test").unwrap();
        assert_eq!(editor.get_text(), "Hello world");
    }

    #[test]
    fn test_version_history() {
        let mut editor = TransactionalCrdtEditor::new("Hello");

        // Make several changes
        for i in 1..=3 {
            editor
                .begin_transaction(format!("t{}", i), format!("Transaction {}", i))
                .unwrap();
            editor
                .insert_in_transaction(
                    &format!("t{}", i),
                    editor.get_text().len(),
                    &format!(" {}", i),
                )
                .unwrap();
            editor.commit_transaction(&format!("t{}", i)).unwrap();
        }

        assert_eq!(editor.get_version(), 3);
        assert_eq!(editor.get_text(), "Hello 1 2 3");

        // Rollback to version 1
        editor.rollback_to_version(1).unwrap();
        assert_eq!(editor.get_version(), 1);
        assert_eq!(editor.get_text(), "Hello 1");
    }

    #[test]
    fn test_fork() {
        let mut editor1 = TransactionalCrdtEditor::new("Hello");

        // Make a change and commit
        editor1
            .begin_transaction("t1".to_string(), "First change".to_string())
            .unwrap();
        editor1.insert_in_transaction("t1", 5, " world").unwrap();
        editor1.commit_transaction("t1").unwrap();

        // Fork the editor
        let mut editor2 = editor1.fork();

        // Make different changes in each fork
        editor1
            .begin_transaction("t2".to_string(), "Change in original".to_string())
            .unwrap();
        editor1.insert_in_transaction("t2", 0, "Say: ").unwrap();
        editor1.commit_transaction("t2").unwrap();

        editor2
            .begin_transaction("t3".to_string(), "Change in fork".to_string())
            .unwrap();
        editor2
            .insert_in_transaction("t3", editor2.get_text().len(), "!")
            .unwrap();
        editor2.commit_transaction("t3").unwrap();

        assert_eq!(editor1.get_text(), "Say: Hello world");
        assert_eq!(editor2.get_text(), "Hello world!");
    }
}
