use anyhow::Result;
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
    /// The source code
    source: &'a str,
}

impl<'a> TargetMap<'a> {
    /// Build a target map from a tree with a single traversal
    pub fn build(tree: &'a Tree, source: &'a str) -> Result<Self> {
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
                        package_name = extract_package_name(&node, source)?;
                    }
                }

                // Check for mantra comment
                "comment" => {
                    if let Ok(text) = node.utf8_text(source.as_bytes()) {
                        let text = text.trim();
                        if text.starts_with("// mantra:") {
                            let instruction = text.strip_prefix("// mantra:").unwrap().trim();
                            pending_instruction = Some(instruction.to_string());
                        }
                    }
                }

                // Check for function/method declaration
                "function_declaration" | "method_declaration" => {
                    // If we have a pending mantra instruction, this function is a target
                    if let Some(instruction) = pending_instruction.take() {
                        if let Ok(target) = parse_function_as_target(&node, source, instruction) {
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

        Ok(Self {
            map,
            package_name,
            source,
        })
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

    /// Get the number of targets
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if the map is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Get source code
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Get package name
    pub fn package_name(&self) -> &str {
        &self.package_name
    }
}

/// Extract package name from package clause node
fn extract_package_name(node: &Node, source: &str) -> Result<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "package_identifier" {
            return Ok(child.utf8_text(source.as_bytes())?.to_string());
        }
    }
    anyhow::bail!("Package name not found")
}

/// Parse a function/method declaration as a Target
fn parse_function_as_target(node: &Node, source: &str, instruction: String) -> Result<Target> {
    // Extract function name
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("unknown")
        .to_string();

    // Extract function signature
    let signature = extract_function_signature(node, source)?;

    Ok(Target {
        name,
        instruction,
        signature,
    })
}

/// Extract the full function signature
fn extract_function_signature(node: &Node, source: &str) -> Result<String> {
    // Get the text from function start to body start
    if let Some(body_node) = node.child_by_field_name("body") {
        let sig_start = node.start_byte();
        let sig_end = body_node.start_byte();
        let signature = &source[sig_start..sig_end];
        Ok(signature.trim().to_string())
    } else {
        // If no body, get the entire function declaration
        Ok(node.utf8_text(source.as_bytes())?.to_string())
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

        let target_map = TargetMap::build(&tree, source).unwrap();

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

        let target_map = TargetMap::build(&tree, source).unwrap();

        assert_eq!(target_map.len(), 1);
        assert_eq!(target_map.package_name(), "service");

        let target = target_map.targets().next().unwrap();
        assert_eq!(target.name, "Process");
        assert_eq!(target.instruction, "Process data");
    }
}
