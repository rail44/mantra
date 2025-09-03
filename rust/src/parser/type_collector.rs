use crop::Rope;
use tree_sitter::Node;

use crate::parser::ast_utils::build_path_to_node;
use crate::parser::target::TypeReference;

/// Collect type references from a function/method declaration
pub fn collect_function_types(
    func_node: &Node,
    tree: &tree_sitter::Tree,
    rope: &Rope,
) -> Vec<TypeReference> {
    let root_node = tree.root_node();
    let mut type_references = Vec::new();

    // For method declarations, collect receiver type
    if func_node.kind() == "method_declaration" {
        if let Some(receiver_list) = func_node.child_by_field_name("receiver") {
            collect_types_from_node(&receiver_list, &root_node, rope, &mut type_references);
        }
    }

    // Collect parameter types
    if let Some(params) = func_node.child_by_field_name("parameters") {
        collect_types_from_node(&params, &root_node, rope, &mut type_references);
    }

    // Collect return types
    if let Some(result) = func_node.child_by_field_name("result") {
        collect_types_from_node(&result, &root_node, rope, &mut type_references);
    }

    type_references
}

/// Recursively collect type nodes and create `TypeReference` objects
pub fn collect_types_from_node(
    node: &Node,
    root_node: &Node,
    rope: &Rope,
    type_references: &mut Vec<TypeReference>,
) {
    match node.kind() {
        "type_identifier" | "pointer_type" | "slice_type" | "array_type" | "channel_type"
        | "map_type" => {
            // Build path from root to this type node
            let path = build_path_to_node(node, root_node);
            // Extract type name from the entire type node (preserves modifiers like *, [])
            let scope_id = rope
                .byte_slice(node.start_byte()..node.end_byte())
                .to_string();
            type_references.push(TypeReference { path, scope_id });
        }
        "qualified_type" => {
            // For qualified types like time.Duration, get the entire qualified type
            let path = build_path_to_node(node, root_node);
            let scope_id = rope
                .byte_slice(node.start_byte()..node.end_byte())
                .to_string();
            type_references.push(TypeReference { path, scope_id });
        }
        _ => {
            // Recursively check children for other node types
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_types_from_node(&child, root_node, rope, type_references);
            }
        }
    }
}
