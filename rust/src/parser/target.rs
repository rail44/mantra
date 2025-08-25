use serde::{Deserialize, Serialize};

use crate::editor::crdt::Snapshot;

/// Information about a parsed Go file
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub package_name: String,
    pub imports: Vec<Import>,
    pub targets: Vec<Target>,
}

/// Import statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub path: String,
    pub alias: Option<String>,
}

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
