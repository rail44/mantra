use crate::editor::crdt::Snapshot;
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
    pub snapshot: Snapshot,
    pub byte_range: Range<usize>,
    /// Type references found in the function signature
    pub type_references: Vec<TypeReference>,
    /// Whether the function body contains panic("not implemented")
    pub has_panic_not_implemented: bool,
}

impl Target {
    /// Find all targets from text (parses with tree-sitter internally)
    pub fn find_targets_from_text(text: &str, uri: &str) -> Vec<Target> {
        use crate::editor::crdt::CrdtEditor;

        let editor = match CrdtEditor::new(text) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        let tree = match editor.tree() {
            Some(t) => t,
            None => return Vec::new(),
        };

        let rope = editor.rope();
        let snapshot = editor.fork();

        Self::find_targets(tree, rope, &snapshot, uri)
    }

    /// Find all targets (functions with mantra comments) in a parsed tree
    pub fn find_targets(tree: &Tree, rope: &Rope, snapshot: &Snapshot, uri: &str) -> Vec<Target> {
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
                        let target = create_target_from_function(
                            &node,
                            tree,
                            rope,
                            snapshot,
                            uri,
                            &instruction,
                        );
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

/// Extract mantra instruction from a comment node
fn extract_mantra_instruction(node: &Node, rope: &Rope) -> Option<String> {
    let text = rope
        .byte_slice(node.start_byte()..node.end_byte())
        .to_string();
    let text = text.trim();
    if text.starts_with("// mantra:") {
        let instruction = text.strip_prefix("// mantra:").unwrap().trim();
        Some(instruction.to_string())
    } else {
        None
    }
}

/// Create a Target from a function/method node
fn create_target_from_function(
    node: &Node,
    tree: &Tree,
    rope: &Rope,
    snapshot: &Snapshot,
    uri: &str,
    instruction: &str,
) -> Target {
    // Extract signature and check for panic("not implemented")
    let (signature, has_panic_not_implemented) = if let Some(body_node) = node.child_by_field_name("body") {
        let sig_start = node.start_byte();
        let sig_end = body_node.start_byte();
        let sig = rope.byte_slice(sig_start..sig_end)
            .to_string()
            .trim()
            .to_string();

        // Check function body for panic("not implemented")
        let body_text = rope.byte_slice(body_node.start_byte()..body_node.end_byte()).to_string();
        let has_panic = body_text.contains("panic(\"not implemented\")");

        (sig, has_panic)
    } else {
        (rope.byte_slice(node.start_byte()..node.end_byte()).to_string(), false)
    };

    // Collect type references
    let type_references = collect_function_types(node, tree, rope);

    // Create the base target for checksum calculation
    let base_target = Target {
        uri: uri.to_string(),
        instruction: instruction.to_string(),
        signature: signature.clone(),
        checksum: 0, // Will be calculated next
        snapshot: snapshot.clone(),
        byte_range: node.start_byte()..node.end_byte(),
        type_references,
        has_panic_not_implemented,
    };

    // Calculate checksum
    let checksum = calculate_checksum(&base_target);

    Target {
        checksum,
        ..base_target
    }
}
