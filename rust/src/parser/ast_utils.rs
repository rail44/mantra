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

    for child in parent.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
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
                    found_field = parent.field_name_for_child(i as u32);
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
                field_name: found_field.map(|s| s.to_string()),
                index: same_kind_index,
            });
        }

        current = parent_opt;
    }

    path.reverse();
    path
}
