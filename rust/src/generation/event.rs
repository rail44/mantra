use crate::editor::crdt::CrdtEditor;

/// Edit event that describes a change to apply to the source
/// Uses mantra checksum as a stable identifier
#[derive(Debug, Clone)]
pub struct EditEvent {
    /// Checksum of the target (stable identifier)
    pub checksum: u64,

    /// Function signature for finding the node in tree-sitter
    pub signature: String,

    /// New body content to replace
    pub new_body: String,

    /// Snapshot of the document when generation task started
    pub snapshot: CrdtEditor,

    /// Function start position (byte offset)
    pub start_byte: usize,

    /// Function end position (byte offset)
    pub end_byte: usize,
}

impl EditEvent {
    pub fn new(
        checksum: u64,
        signature: String,
        new_body: String,
        snapshot: CrdtEditor,
        start_byte: usize,
        end_byte: usize,
    ) -> Self {
        Self {
            checksum,
            signature,
            new_body,
            snapshot,
            start_byte,
            end_byte,
        }
    }
}
