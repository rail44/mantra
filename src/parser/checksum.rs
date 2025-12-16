use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

use super::target::Target;

/// Prefix for checksum comments
pub const CHECKSUM_PREFIX: &str = "// mantra:checksum:";

/// Calculate checksum for a target to detect changes
/// Uses `FxHasher` for fast, deterministic hashing
pub fn calculate_checksum(target: &Target) -> u64 {
    let mut hasher = FxHasher::default();

    // Hash the signature and instruction
    target.signature.hash(&mut hasher);
    target.instruction.hash(&mut hasher);

    let checksum = hasher.finish();

    tracing::debug!(
        checksum = format!("{:x}", checksum),
        signature = %target.signature,
        instruction = %target.instruction,
        "Calculated checksum"
    );

    checksum
}

/// Extract checksum from text that starts with the checksum prefix
/// Returns None if the text doesn't start with the prefix or parsing fails
pub fn extract_checksum_from_text(text: &str) -> Option<u64> {
    let text = text.trim();
    text.strip_prefix(CHECKSUM_PREFIX).and_then(|s| {
        let hex_str = s.split_whitespace().next().unwrap_or(s.trim());
        u64::from_str_radix(hex_str, 16).ok()
    })
}
