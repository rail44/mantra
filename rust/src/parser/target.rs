use crate::parser::checksum::calculate_checksum;
use crate::parser::type_collector::collect_function_types;
use crop::Rope;
use std::ops::Range;
use tree_sitter::{Node, Tree};

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
    pub byte_range: Range<usize>,
    /// Type references found in the function signature
    pub type_references: Vec<TypeReference>,
    /// Whether this target has already been generated (checksum comment exists)
    pub is_generated: bool,
    /// Range of the checksum comment if it exists (for extracting full generated text)
    pub checksum_comment_range: Option<Range<usize>>,
}

impl Target {
    /// Find all targets (functions with mantra comments) in a parsed tree
    pub fn find_targets(tree: &Tree, rope: &Rope, uri: &str) -> Vec<Target> {
        // First pass: collect all existing checksum comments
        let existing_checksums = collect_existing_checksums(tree, rope);

        // Second pass: find targets
        let mut targets = Vec::new();
        let mut pending_instruction: Option<String> = None;
        let mut stack = vec![tree.root_node()];

        while let Some(node) = stack.pop() {
            match node.kind() {
                "comment" => {
                    if let Some(instruction) = extract_mantra_instruction(&node, rope) {
                        pending_instruction = Some(instruction);
                    }
                }

                "function_declaration" | "method_declaration" => {
                    if let Some(instruction) = pending_instruction.take() {
                        let mut target =
                            create_target_from_function(&node, tree, rope, uri, &instruction);
                        // Check if this target's checksum already exists and get comment range
                        if let Some(comment_range) = existing_checksums.get(&target.checksum) {
                            target.is_generated = true;
                            target.checksum_comment_range = Some(comment_range.clone());
                        }
                        targets.push(target);
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

        targets
    }
}

/// Collect all existing checksum comments from the tree
/// Returns a map of checksum -> comment byte range
fn collect_existing_checksums(
    tree: &Tree,
    rope: &Rope,
) -> rustc_hash::FxHashMap<u64, Range<usize>> {
    let mut checksums = rustc_hash::FxHashMap::default();
    let mut stack = vec![tree.root_node()];

    while let Some(node) = stack.pop() {
        if node.kind() == "comment" {
            if let Some(checksum) = extract_checksum_comment(&node, rope) {
                checksums.insert(checksum, node.start_byte()..node.end_byte());
            }
        }

        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }

    checksums
}

/// Extract mantra instruction from a comment node
/// Returns None for checksum comments (// mantra:checksum:xxx)
fn extract_mantra_instruction(node: &Node, rope: &Rope) -> Option<String> {
    let text = rope
        .byte_slice(node.start_byte()..node.end_byte())
        .to_string();
    let text = text.trim();
    if text.starts_with("// mantra:") {
        let instruction = text.strip_prefix("// mantra:").unwrap().trim();
        // Skip checksum comments - they indicate already generated code
        if instruction.starts_with("checksum:") {
            return None;
        }
        Some(instruction.to_string())
    } else {
        None
    }
}

/// Extract checksum from a mantra checksum comment
fn extract_checksum_comment(node: &Node, rope: &Rope) -> Option<u64> {
    let text = rope
        .byte_slice(node.start_byte()..node.end_byte())
        .to_string();
    let text = text.trim();
    if text.starts_with("// mantra:checksum:") {
        let checksum_str = text.strip_prefix("// mantra:checksum:").unwrap().trim();
        u64::from_str_radix(checksum_str, 16).ok()
    } else {
        None
    }
}

/// Create a Target from a function/method node
fn create_target_from_function(
    node: &Node,
    tree: &Tree,
    rope: &Rope,
    uri: &str,
    instruction: &str,
) -> Target {
    // Extract signature
    let signature = if let Some(body_node) = node.child_by_field_name("body") {
        let sig_start = node.start_byte();
        let sig_end = body_node.start_byte();
        rope.byte_slice(sig_start..sig_end)
            .to_string()
            .trim()
            .to_string()
    } else {
        rope.byte_slice(node.start_byte()..node.end_byte())
            .to_string()
    };

    // Collect type references
    let type_references = collect_function_types(node, tree, rope);

    // byte_range is the function only (not including the mantra comment)
    let byte_range = node.start_byte()..node.end_byte();

    // Create the base target for checksum calculation
    let base_target = Target {
        uri: uri.to_string(),
        instruction: instruction.to_string(),
        signature: signature.clone(),
        checksum: 0, // Will be calculated next
        byte_range,
        type_references,
        is_generated: false,          // Will be set later in find_targets
        checksum_comment_range: None, // Will be set later in find_targets if exists
    };

    // Calculate checksum
    let checksum = calculate_checksum(&base_target);

    Target {
        checksum,
        ..base_target
    }
}
