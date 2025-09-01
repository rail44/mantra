use crate::editor::crdt::Snapshot;
use std::ops::Range;

/// A segment in an AST path for locating a node
#[derive(Debug, Clone)]
pub struct PathSegment {
    /// The kind of node (e.g., "`type_identifier`", "`parameter_list`")
    pub node_kind: String,
    /// Optional field name if this node is accessed by field
    pub field_name: Option<String>,
    /// Optional index if multiple nodes of same kind exist
    pub index: Option<usize>,
}

/// Reference to a type in the code with its AST path and identifier
#[derive(Debug, Clone)]
pub struct TypeReference {
    /// AST path to locate the type node
    pub path: Vec<PathSegment>,
    /// Type name identifier (e.g., "`SimpleCache`", "*User", "[]string")
    /// Can be hierarchical like "SimpleCache.FieldName" for nested inspection
    pub scope_id: String,
}

/// Target function or method to generate
#[derive(Debug, Clone)]
pub struct Target {
    pub uri: String,
    pub instruction: String,
    pub signature: String,
    pub checksum: u64,
    pub snapshot: Snapshot,
    pub byte_range: Range<usize>,
    /// Type references found in the function signature
    pub type_references: Vec<TypeReference>,
}

impl Target {}
