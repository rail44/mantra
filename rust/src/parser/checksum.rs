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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::target::Target;

    #[test]
    fn test_checksum_changes_with_signature() {
        let mut target = Target {
            name: "TestFunc".to_string(),
            instruction: "Test instruction".to_string(),
            signature: "func TestFunc(id string)".to_string(),
            checksum: 0,
            snapshot: crate::editor::crdt::Snapshot {
                replica: cola::Replica::new(1, 0),
                rope: crop::Rope::new(),
                version: 0,
            },
            start_byte: 0,
            end_byte: 0,
        };

        let checksum1 = calculate_checksum(&target);

        // Change signature
        target.signature = "func TestFunc(id int)".to_string();
        let checksum2 = calculate_checksum(&target);

        assert_ne!(
            checksum1, checksum2,
            "Checksum should change when signature changes"
        );
    }

    #[test]
    fn test_checksum_changes_with_instruction() {
        let mut target = Target {
            name: "TestFunc".to_string(),
            instruction: "Original instruction".to_string(),
            signature: "func TestFunc()".to_string(),
            checksum: 0,
            snapshot: crate::editor::crdt::Snapshot {
                replica: cola::Replica::new(1, 0),
                rope: crop::Rope::new(),
                version: 0,
            },
            start_byte: 0,
            end_byte: 0,
        };

        let checksum1 = calculate_checksum(&target);

        // Change instruction
        target.instruction = "Updated instruction".to_string();
        let checksum2 = calculate_checksum(&target);

        assert_ne!(
            checksum1, checksum2,
            "Checksum should change when instruction changes"
        );
    }

    #[test]
    fn test_checksum_stable() {
        let target = Target {
            name: "TestFunc".to_string(),
            instruction: "Test instruction".to_string(),
            signature: "func TestFunc()".to_string(),
            checksum: 0,
            snapshot: crate::editor::crdt::Snapshot {
                replica: cola::Replica::new(1, 0),
                rope: crop::Rope::new(),
                version: 0,
            },
            start_byte: 0,
            end_byte: 0,
        };

        let checksum1 = calculate_checksum(&target);
        let checksum2 = calculate_checksum(&target);

        assert_eq!(
            checksum1, checksum2,
            "Checksum should be stable for same input"
        );
    }
}
