use crate::editor::crdt::Snapshot;
use std::ops::Range;

/// Target function or method to generate
#[derive(Debug, Clone)]
pub struct Target {
    pub instruction: String,
    pub signature: String,
    pub checksum: u64,
    pub snapshot: Snapshot,
    pub byte_range: Range<usize>,
}

impl Target {}
