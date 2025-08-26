use crate::editor::crdt::Snapshot;
use std::ops::Range;

/// A segment in an AST path for locating a node
#[derive(Debug, Clone)]
pub struct PathSegment {
    /// The kind of node (e.g., "type_identifier", "parameter_list")
    pub node_kind: String,
    /// Optional field name if this node is accessed by field
    pub field_name: Option<String>,
    /// Optional index if multiple nodes of same kind exist
    pub index: Option<usize>,
}

/// Target function or method to generate
#[derive(Debug, Clone)]
pub struct Target {
    pub instruction: String,
    pub signature: String,
    pub checksum: u64,
    pub snapshot: Snapshot,
    pub byte_range: Range<usize>,
    /// Paths to type nodes in the AST that should be resolved
    pub type_references: Vec<Vec<PathSegment>>,
}

impl Target {}
