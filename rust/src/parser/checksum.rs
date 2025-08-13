use hashbrown::hash_map::DefaultHashBuilder;
use std::hash::{BuildHasher, Hash, Hasher};

use super::target::Target;

/// Calculate checksum for a target to detect changes
pub fn calculate_checksum(target: &Target) -> u64 {
    let hash_builder = DefaultHashBuilder::default();
    let mut hasher = hash_builder.build_hasher();
    
    // Hash the signature and instruction
    target.signature.hash(&mut hasher);
    target.instruction.hash(&mut hasher);
    
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::target::{Target, Param, Return};
    use std::path::PathBuf;
    
    #[test]
    fn test_checksum_changes_with_signature() {
        let mut target = Target {
            name: "TestFunc".to_string(),
            receiver: None,
            params: vec![
                Param {
                    name: "id".to_string(),
                    typ: "string".to_string(),
                }
            ],
            returns: vec![],
            instruction: "Test instruction".to_string(),
            file_path: PathBuf::from("test.go"),
            has_panic: true,
            signature: "func TestFunc(id string)".to_string(),
        };
        
        let checksum1 = calculate_checksum(&target);
        
        // Change parameter type
        target.params[0].typ = "int".to_string();
        target.signature = "func TestFunc(id int)".to_string();
        let checksum2 = calculate_checksum(&target);
        
        assert_ne!(checksum1, checksum2, "Checksum should change when signature changes");
    }
    
    #[test]
    fn test_checksum_changes_with_instruction() {
        let mut target = Target {
            name: "TestFunc".to_string(),
            receiver: None,
            params: vec![],
            returns: vec![],
            instruction: "Original instruction".to_string(),
            file_path: PathBuf::from("test.go"),
            has_panic: true,
            signature: "func TestFunc()".to_string(),
        };
        
        let checksum1 = calculate_checksum(&target);
        
        // Change instruction
        target.instruction = "Updated instruction".to_string();
        let checksum2 = calculate_checksum(&target);
        
        assert_ne!(checksum1, checksum2, "Checksum should change when instruction changes");
    }
    
    #[test]
    fn test_checksum_stable() {
        let target = Target {
            name: "TestFunc".to_string(),
            receiver: None,
            params: vec![],
            returns: vec![],
            instruction: "Test instruction".to_string(),
            file_path: PathBuf::from("test.go"),
            has_panic: true,
            signature: "func TestFunc()".to_string(),
        };
        
        let checksum1 = calculate_checksum(&target);
        let checksum2 = calculate_checksum(&target);
        
        assert_eq!(checksum1, checksum2, "Checksum should be stable for same input");
    }
}