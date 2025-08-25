use anyhow::Result;
use crop::Rope;
use hashbrown::HashMap;
use tree_sitter::{Node, Tree};

use super::checksum::calculate_checksum;
use super::target::Target;

/// A map of target nodes indexed by their checksum
pub struct TargetMap<'a> {
    /// Map from checksum to (target, node)
    map: HashMap<u64, (Target, Node<'a>)>,
    /// Package name
    package_name: String,
}

impl<'a> TargetMap<'a> {
    /// Build a target map from a tree with a single traversal
    pub fn build(tree: &'a Tree, rope: &Rope) -> Result<Self> {
        let mut map = HashMap::new();
        let mut package_name = String::new();

        // Single pass traversal
        let mut pending_instruction: Option<String> = None;
        let mut stack = vec![tree.root_node()];

        while let Some(node) = stack.pop() {
            match node.kind() {
                // Extract package name
                "package_clause" => {
                    if package_name.is_empty() {
                        package_name = extract_package_name(&node, rope)?;
                    }
                }

                // Check for mantra comment
                "comment" => {
                    let text = rope
                        .byte_slice(node.start_byte()..node.end_byte())
                        .to_string();
                    let text = text.trim();
                    if text.starts_with("// mantra:") {
                        let instruction = text.strip_prefix("// mantra:").unwrap().trim();
                        pending_instruction = Some(instruction.to_string());
                    }
                }

                // Check for function/method declaration
                "function_declaration" | "method_declaration" => {
                    // If we have a pending mantra instruction, this function is a target
                    if let Some(instruction) = pending_instruction.take() {
                        if let Ok(target) = parse_function_as_target(&node, rope, instruction) {
                            let checksum = calculate_checksum(&target);
                            map.insert(checksum, (target, node));
                        }
                    }
                }

                _ => {}
            }

            // Add children to stack in reverse order for depth-first traversal
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }

        Ok(Self { map, package_name })
    }

    /// Get a target and its node by checksum
    pub fn get(&self, checksum: u64) -> Option<&(Target, Node<'a>)> {
        self.map.get(&checksum)
    }

    /// Get all checksums
    pub fn checksums(&self) -> impl Iterator<Item = u64> + '_ {
        self.map.keys().copied()
    }

    /// Get all targets
    pub fn targets(&self) -> impl Iterator<Item = &Target> + '_ {
        self.map.values().map(|(target, _)| target)
    }

    /// Get all function nodes
    pub fn nodes(&self) -> impl Iterator<Item = &Node<'a>> + '_ {
        self.map.values().map(|(_, node)| node)
    }

    /// Iterate over all entries (checksum, target, node)
    pub fn iter(&self) -> impl Iterator<Item = (&u64, &(Target, Node<'a>))> + '_ {
        self.map.iter()
    }

    /// Get the number of targets
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if the map is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Get package name
    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    /// Get all targets with checksum and node
    pub fn all_targets(&self) -> Vec<(u64, Target, Node<'a>)> {
        self.map
            .iter()
            .map(|(checksum, (target, node))| (*checksum, target.clone(), *node))
            .collect()
    }
}

/// Extract package name from package clause node
fn extract_package_name(node: &Node, rope: &Rope) -> Result<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "package_identifier" {
            let text = rope
                .byte_slice(child.start_byte()..child.end_byte())
                .to_string();
            return Ok(text);
        }
    }
    anyhow::bail!("Package name not found")
}

/// Parse a function/method declaration as a Target
fn parse_function_as_target(node: &Node, rope: &Rope, instruction: String) -> Result<Target> {
    // Extract function name
    let name = node
        .child_by_field_name("name")
        .map(|n| rope.byte_slice(n.start_byte()..n.end_byte()).to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Extract function signature
    let signature = extract_function_signature(node, rope)?;

    // This is legacy code - should not be used
    // TODO: Remove TargetMap when no longer needed
    panic!("TargetMap is deprecated - use Document::find_targets instead")
}

/// Extract the full function signature
fn extract_function_signature(node: &Node, rope: &Rope) -> Result<String> {
    // Get the text from function start to body start
    if let Some(body_node) = node.child_by_field_name("body") {
        let sig_start = node.start_byte();
        let sig_end = body_node.start_byte();
        let signature = rope.byte_slice(sig_start..sig_end).to_string();
        Ok(signature.trim().to_string())
    } else {
        // If no body, get the entire function declaration
        let text = rope
            .byte_slice(node.start_byte()..node.end_byte())
            .to_string();
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::GoParser;

    #[test]
    fn test_target_map_single_pass() {
        let source = r#"
package main

// mantra: Add two numbers
func Add(a, b int) int {
    panic("not implemented")
}

// Regular comment
func NotTarget() {}

// mantra: Multiply two numbers
func Multiply(x, y int) int {
    panic("not implemented")
}
"#;

        let mut parser = GoParser::new().unwrap();
        let tree = parser.parse(source).unwrap();
        let rope = Rope::from(source);

        let target_map = TargetMap::build(&tree, &rope).unwrap();

        assert_eq!(target_map.len(), 2);
        assert_eq!(target_map.package_name(), "main");

        // Check targets
        let targets: Vec<_> = target_map.targets().collect();
        assert_eq!(targets.len(), 2);

        // Verify target details
        for target in targets {
            assert!(target.name == "Add" || target.name == "Multiply");
            assert!(!target.instruction.is_empty());
            assert!(!target.signature.is_empty());
        }
    }

    #[test]
    fn test_target_map_with_methods() {
        let source = r#"
package service

type Service struct{}

// mantra: Process data
func (s *Service) Process(data string) error {
    panic("not implemented")
}
"#;

        let mut parser = GoParser::new().unwrap();
        let tree = parser.parse(source).unwrap();
        let rope = Rope::from(source);

        let target_map = TargetMap::build(&tree, &rope).unwrap();

        assert_eq!(target_map.len(), 1);
        assert_eq!(target_map.package_name(), "service");

        let target = target_map.targets().next().unwrap();
        assert_eq!(target.name, "Process");
        assert_eq!(target.instruction, "Process data");
    }
}
