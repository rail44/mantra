use crop::Rope;
use tree_sitter::Node;

use crate::parser::target::PathSegment;

/// Find a node in the AST by following a path of segments
pub fn find_node_by_path<'a>(root: &Node<'a>, path: &[PathSegment]) -> Option<Node<'a>> {
    if path.is_empty() {
        return None;
    }

    let mut current = *root;

    for (depth, segment) in path.iter().enumerate() {
        // Skip root node check
        if depth == 0 && segment.node_kind == "source_file" {
            continue;
        }

        // Find next node based on segment
        let next_node = if let Some(field_name) = &segment.field_name {
            // Prefer field name lookup (most stable)
            current
                .child_by_field_name(field_name)
                .filter(|n| n.kind() == segment.node_kind)
        } else if let Some(index) = segment.index {
            // Find nth child of same kind
            find_nth_child_of_kind(&current, &segment.node_kind, index)
        } else {
            // Find first child of kind
            find_first_child_of_kind(&current, &segment.node_kind)
        };

        match next_node {
            Some(node) => current = node,
            None => return None,
        }
    }

    Some(current)
}

/// Find the nth child node of a specific kind
fn find_nth_child_of_kind<'a>(
    parent: &Node<'a>,
    kind: &str,
    target_index: usize,
) -> Option<Node<'a>> {
    let mut count = 0;
    let mut cursor = parent.walk();

    for child in parent.children(&mut cursor) {
        if child.kind() == kind {
            if count == target_index {
                return Some(child);
            }
            count += 1;
        }
    }
    None
}

/// Find the first child node of a specific kind
fn find_first_child_of_kind<'a>(parent: &Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = parent.walk();
    let result = parent
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    result
}

/// Find a symbol (field or method) within a node with text comparison
pub fn find_symbol_in_node<'a>(
    parent: &Node<'a>,
    symbol_name: &str,
    rope: &crop::Rope,
) -> Option<Node<'a>> {
    // Search through different node types that might contain symbols
    match parent.kind() {
        // For struct types, look in field_declaration_list
        "struct_type" => {
            // Look for field_declaration_list among children
            let mut cursor = parent.walk();
            for child in parent.children(&mut cursor) {
                if child.kind() == "field_declaration_list" {
                    return find_field_in_list(&child, symbol_name, rope);
                }
            }
            None
        }
        // For interface types, look in method_spec_list
        "interface_type" => {
            if let Some(method_list) = parent.child_by_field_name("methods") {
                find_method_in_list(&method_list, symbol_name, rope)
            } else {
                None
            }
        }
        // For type_spec, look inside the type definition
        "type_spec" => {
            if let Some(type_node) = parent.child_by_field_name("type") {
                find_symbol_in_node(&type_node, symbol_name, rope)
            } else {
                None
            }
        }
        // For qualified_type (like time.Duration), extract the definition target
        "qualified_type" => {
            let node_text = rope
                .byte_slice(parent.start_byte()..parent.end_byte())
                .to_string();
            if node_text == symbol_name {
                extract_definition_target_from_qualified(parent)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Find a field in a `field_declaration_list`
fn find_field_in_list<'a>(
    field_list: &Node<'a>,
    field_name: &str,
    rope: &crop::Rope,
) -> Option<Node<'a>> {
    let mut cursor = field_list.walk();

    for child in field_list.children(&mut cursor) {
        if child.kind() == "field_declaration" {
            // Search within this field_declaration
            if let Some(node) = find_matching_node_in_field(&child, field_name, rope) {
                return Some(node);
            }
        }
    }
    None
}

/// Find a matching node within a field declaration
fn find_matching_node_in_field<'a>(
    parent: &Node<'a>,
    target_text: &str,
    rope: &crop::Rope,
) -> Option<Node<'a>> {
    let mut cursor = parent.walk();
    for node in parent.children(&mut cursor) {
        let node_text = rope
            .byte_slice(node.start_byte()..node.end_byte())
            .to_string();

        if node_text == target_text {
            // Check if it's a field_identifier (direct match)
            if node.kind() == "field_identifier" {
                return Some(node);
            }
            // Otherwise try to extract from qualified identifier
            return extract_definition_target_from_qualified(&node);
        }

        // Recursively search in children
        if let Some(result) = find_matching_node_in_field(&node, target_text, rope) {
            return Some(result);
        }
    }
    None
}

/// Extract the definition target node from a qualified identifier based on AST structure
pub fn extract_definition_target_from_qualified<'a>(qualified_node: &Node<'a>) -> Option<Node<'a>> {
    match qualified_node.kind() {
        // For qualified_type nodes (like time.Duration), return the type identifier (right side)
        "qualified_type" => {
            if let Some(name_node) = qualified_node.child_by_field_name("name") {
                Some(name_node)
            } else {
                // Fallback: get the rightmost identifier child
                let mut cursor = qualified_node.walk();
                qualified_node
                    .children(&mut cursor)
                    .filter(|child| child.kind() == "type_identifier")
                    .last()
            }
        }
        // For selector_expression nodes (like pkg.Function), return the field (right side)
        "selector_expression" => qualified_node.child_by_field_name("field"),
        // For other qualified structures, try to find the rightmost identifier
        _ => {
            let mut cursor = qualified_node.walk();
            qualified_node
                .children(&mut cursor)
                .filter(|child| child.kind().contains("identifier"))
                .last()
        }
    }
}

/// Find a method in a `method_spec_list`
fn find_method_in_list<'a>(
    method_list: &Node<'a>,
    method_name: &str,
    rope: &crop::Rope,
) -> Option<Node<'a>> {
    let mut cursor = method_list.walk();

    for child in method_list.children(&mut cursor) {
        if child.kind() == "method_spec" {
            // Check the name field specifically
            if let Some(name_node) = child.child_by_field_name("name") {
                let method_text = rope
                    .byte_slice(name_node.start_byte()..name_node.end_byte())
                    .to_string();
                if method_text == method_name {
                    return Some(name_node);
                }
            }
        }
    }
    None
}

/// Build a path from root to a given node
pub fn build_path_to_node(target: &Node, root: &Node) -> Vec<PathSegment> {
    let mut path = Vec::new();
    let mut current = Some(*target);

    // Build path from target to root
    while let Some(node) = current {
        if node.id() == root.id() {
            // Reached root
            path.push(PathSegment {
                node_kind: node.kind().to_string(),
                field_name: None,
                index: None,
            });
            break;
        }

        let parent_opt = node.parent();
        if let Some(parent) = parent_opt {
            // Find how this node is referenced from parent
            let mut found_field = None;
            let mut same_kind_index = None;
            let mut cursor = parent.walk();

            // Count nodes of same kind before this one
            let mut same_kind_count = 0;
            for (i, child) in parent.children(&mut cursor).enumerate() {
                if child.id() == node.id() {
                    // Found our node - check if it has a field name
                    found_field = parent.field_name_for_child(u32::try_from(i).unwrap());
                    if found_field.is_none() && same_kind_count > 0 {
                        same_kind_index = Some(same_kind_count);
                    }
                    break;
                } else if child.kind() == node.kind() {
                    same_kind_count += 1;
                }
            }

            path.push(PathSegment {
                node_kind: node.kind().to_string(),
                field_name: found_field.map(std::string::ToString::to_string),
                index: same_kind_index,
            });
        }

        current = parent_opt;
    }

    path.reverse();
    path
}

/// Find a node at a specific byte position
pub fn find_node_at_byte_position<'a>(root: &Node<'a>, byte_pos: usize) -> Option<Node<'a>> {
    root.descendant_for_byte_range(byte_pos, byte_pos)
}

/// Find a function or method declaration at a specific byte position
/// Returns the function/method node if found, or None if the position is not within a function
pub fn find_function_at_byte_position<'a>(root: &Node<'a>, byte_pos: usize) -> Option<Node<'a>> {
    let node = root.descendant_for_byte_range(byte_pos, byte_pos)?;
    find_parent_function(node)
}

/// Walk up the tree to find a function or method declaration
fn find_parent_function(start_node: Node) -> Option<Node> {
    let mut current = Some(start_node);

    while let Some(node) = current {
        match node.kind() {
            "function_declaration" | "method_declaration" => {
                return Some(node);
            }
            _ => {
                current = node.parent();
            }
        }
    }

    None
}

/// Find a definition node starting from a given node
/// In Go, we're looking for `type_spec`, `const_spec`, `var_spec`, `function_declaration`, `method_declaration`
pub fn find_definition_node(start_node: Node) -> Option<(Node, (usize, usize))> {
    let mut current = Some(start_node);

    while let Some(n) = current {
        match n.kind() {
            // Type, constant, variable, method spec, and field definitions
            "type_spec" | "type_declaration" | "const_spec" | "const_declaration" | "var_spec"
            | "var_declaration" | "method_spec" | "field_declaration" => {
                return Some((n, (n.start_byte(), n.end_byte())));
            }
            // Function/method definitions
            "function_declaration" | "method_declaration" => {
                // For functions, we typically want just the signature, not the body
                if let Some(params) = n.child_by_field_name("parameters") {
                    // Get from start of function to end of parameters
                    return Some((n, (n.start_byte(), params.end_byte())));
                }
                return Some((n, (n.start_byte(), n.end_byte())));
            }
            _ => {
                current = n.parent();
            }
        }
    }

    None
}

/// Extract definition content from a node or an identifier
pub fn extract_definition_content(
    node: Node,
    rope: &Rope,
    root: &Node,
) -> Option<(String, Vec<PathSegment>)> {
    // Try to find a definition node first
    if let Some((def_node, (start, end))) = find_definition_node(node) {
        let content = rope.byte_slice(start..end).to_string();
        let path_segments = build_path_to_node(&def_node, root);
        Some((content, path_segments))
    } else if node.kind() == "type_identifier" || node.kind() == "identifier" {
        // If we couldn't find a definition node, the position might be pointing
        // to an identifier that is the definition itself
        let content = rope
            .byte_slice(node.start_byte()..node.end_byte())
            .to_string();
        let path_segments = build_path_to_node(&node, root);
        Some((content, path_segments))
    } else {
        None
    }
}

/// Get the target node for definition lookup
/// Handles special cases like `qualified_type` and `slice_type`
pub fn get_definition_target_node<'a>(
    node: Node<'a>,
    symbol_name: Option<&str>,
    rope: &Rope,
) -> Node<'a> {
    if let Some(symbol) = symbol_name {
        find_symbol_in_node(&node, symbol, rope).unwrap_or(node)
    } else {
        // For qualified_type nodes, use the definition target position
        if node.kind() == "qualified_type" {
            extract_definition_target_from_qualified(&node).unwrap_or(node)
        } else if node.kind() == "slice_type" {
            // For slice types like []string, find the element type
            node.child_by_field_name("element").unwrap_or(node)
        } else {
            node
        }
    }
}
