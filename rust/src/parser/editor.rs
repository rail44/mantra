use tree_sitter::{Node, Tree};

/// Function info for editing
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub start_byte: usize,
    pub body_start: usize,
    pub body_end: usize,
}

/// Find a function by signature in the tree and return its info
pub fn find_function_info(source: &str, tree: &Tree, signature: &str) -> Option<FunctionInfo> {
    let root = tree.root_node();
    find_function_in_node(source, &root, signature).map(|node| extract_function_info(&node))
}

/// Extract function info from node
fn extract_function_info(node: &Node) -> FunctionInfo {
    let body_node = node.child_by_field_name("body");

    FunctionInfo {
        start_byte: node.start_byte(),
        body_start: body_node
            .as_ref()
            .map(|n| n.start_byte())
            .unwrap_or(node.end_byte()),
        body_end: body_node
            .as_ref()
            .map(|n| n.end_byte())
            .unwrap_or(node.end_byte()),
    }
}

fn find_function_in_node<'a>(source: &str, node: &Node<'a>, signature: &str) -> Option<Node<'a>> {
    // Check if this is a function with matching signature
    if node.kind() == "function_declaration" || node.kind() == "method_declaration" {
        let sig = build_signature(source, node);
        if sig == signature {
            return Some(*node);
        }
    }

    // Check children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_function_in_node(source, &child, signature) {
            return Some(found);
        }
    }

    None
}

/// Build signature string from function node
fn build_signature(source: &str, node: &Node) -> String {
    // Extract the signature text from source
    let start = node.start_byte();
    let mut end = node.end_byte();

    // Find the opening brace to get just the signature
    if let Some(body) = node.child_by_field_name("body") {
        end = body.start_byte();
    }

    source[start..end]
        .trim_end()
        .trim_end_matches('{')
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::GoParser;

    #[test]
    fn test_find_function_info() {
        let source = r#"package main

func Add(a, b int) int {
    panic("not implemented")
}"#;

        let mut parser = GoParser::new().unwrap();
        let tree = parser.parse(source).unwrap();

        // Find the function
        let func_info = find_function_info(source, &tree, "func Add(a, b int) int").unwrap();

        // Check function info
        assert!(func_info.body_start > func_info.start_byte);
        assert!(func_info.body_end > func_info.body_start);
    }
}
