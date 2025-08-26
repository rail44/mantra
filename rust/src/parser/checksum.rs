use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

use super::target::Target;

/// Calculate checksum for a target to detect changes
/// Uses FxHasher for fast, deterministic hashing
pub fn calculate_checksum(target: &Target) -> u64 {
    let mut hasher = FxHasher::default();

    // Hash the signature and instruction
    target.signature.hash(&mut hasher);
    target.instruction.hash(&mut hasher);

    hasher.finish()
}
