use crate::editor::crdt::Snapshot;

/// Target function or method to generate
#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    pub instruction: String,
    pub signature: String,
    pub checksum: u64,
    pub snapshot: Snapshot,
    pub start_byte: usize,
    pub end_byte: usize,
}

impl Target {}
