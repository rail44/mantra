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
        _ => None,
    }
}

/// Find a field in a field_declaration_list
fn find_field_in_list<'a>(
    field_list: &Node<'a>,
    field_name: &str,
    rope: &crop::Rope,
) -> Option<Node<'a>> {
    let mut cursor = field_list.walk();

    for child in field_list.children(&mut cursor) {
        if child.kind() == "field_declaration" {
            // Look for field_identifier within this field_declaration
            let mut field_cursor = child.walk();
            for field_child in child.children(&mut field_cursor) {
                if field_child.kind() == "field_identifier" {
                    // Get the text content and compare
                    let field_text = rope
                        .byte_slice(field_child.start_byte()..field_child.end_byte())
                        .to_string();
                    if field_text == field_name {
                        return Some(field_child);
                    }
                }
            }
        }
    }
    None
}

/// Find a method in a method_spec_list
fn find_method_in_list<'a>(
    method_list: &Node<'a>,
    method_name: &str,
    rope: &crop::Rope,
) -> Option<Node<'a>> {
    let mut cursor = method_list.walk();

    for child in method_list.children(&mut cursor) {
        if child.kind() == "method_spec" {
            // Look for method name within this method_spec
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
