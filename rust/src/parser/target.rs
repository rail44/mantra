use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tree_sitter::{Node, Tree, TreeCursor};

/// Maximum allowed gap between a mantra comment and its target function
const MAX_COMMENT_GAP: usize = 50;

/// Information about a parsed Go file
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub package_name: String,
    pub imports: Vec<Import>,
    pub targets: Vec<Target>,
}

/// Import statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub path: String,
    pub alias: Option<String>,
}

/// Target function or method to generate
#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    pub instruction: String,
    pub signature: String,
}

impl Target {}

/// Extract file information from parsed tree
pub fn extract_file_info(path: &Path, source: &str, tree: Tree) -> Result<FileInfo> {
    let mut file_info = FileInfo {
        package_name: String::new(),
        imports: Vec::new(),
        targets: Vec::new(),
    };

    let root_node = tree.root_node();

    // Extract package name
    file_info.package_name = extract_package_name(&root_node, source)?;

    // Extract imports
    file_info.imports = extract_imports(&root_node, source);

    // Extract mantra targets
    file_info.targets = extract_targets(&root_node, source, path)?;

    Ok(file_info)
}

/// Extract package name from source
fn extract_package_name(root: &Node, source: &str) -> Result<String> {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "package_clause" {
            // tree-sitter-go uses package_identifier as a child node, not a field
            let mut pkg_cursor = child.walk();
            for grandchild in child.children(&mut pkg_cursor) {
                if grandchild.kind() == "package_identifier" {
                    return Ok(grandchild.utf8_text(source.as_bytes())?.to_string());
                }
            }
        }
    }

    anyhow::bail!("Package name not found")
}

/// Extract imports from source
fn extract_imports(root: &Node, source: &str) -> Vec<Import> {
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_declaration" {
            // Handle both single and grouped imports
            extract_import_specs(&child, source, &mut imports);
        }
    }

    imports
}

/// Extract import specs from import declaration
fn extract_import_specs(node: &Node, source: &str, imports: &mut Vec<Import>) {
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "import_spec_list" {
            // Grouped imports
            let mut spec_cursor = child.walk();
            for spec in child.children(&mut spec_cursor) {
                if spec.kind() == "import_spec" {
                    if let Some(import) = parse_import_spec(&spec, source) {
                        imports.push(import);
                    }
                }
            }
        } else if child.kind() == "import_spec" {
            // Single import
            if let Some(import) = parse_import_spec(&child, source) {
                imports.push(import);
            }
        }
    }
}

/// Parse a single import spec
fn parse_import_spec(spec: &Node, source: &str) -> Option<Import> {
    let mut path = String::new();
    let mut alias = None;
    let mut cursor = spec.walk();

    for child in spec.children(&mut cursor) {
        match child.kind() {
            "interpreted_string_literal" => {
                if let Ok(text) = child.utf8_text(source.as_bytes()) {
                    // Remove quotes
                    path = text.trim_matches('"').to_string();
                }
            }
            "identifier" | "blank_identifier" => {
                if let Ok(text) = child.utf8_text(source.as_bytes()) {
                    alias = Some(text.to_string());
                }
            }
            _ => {}
        }
    }

    if !path.is_empty() {
        Some(Import { path, alias })
    } else {
        None
    }
}

/// Extract mantra targets from source
fn extract_targets(root: &Node, source: &str, file_path: &Path) -> Result<Vec<Target>> {
    let mut targets = Vec::new();
    let mut mantra_comments = extract_mantra_comments(root, source)?;

    // Find functions with mantra comments
    extract_functions_with_mantra(root, source, file_path, &mut mantra_comments, &mut targets)?;

    Ok(targets)
}

/// Extract all mantra comments from the source
fn extract_mantra_comments(root: &Node, source: &str) -> Result<Vec<(usize, String)>> {
    let mut comments = Vec::new();
    let mut cursor = root.walk();

    visit_nodes(&mut cursor, &mut |node| {
        if node.kind() == "comment" {
            if let Ok(text) = node.utf8_text(source.as_bytes()) {
                let text = text.trim();
                if text.starts_with("// mantra:") {
                    let instruction = text.strip_prefix("// mantra:").unwrap().trim();
                    let end_byte = node.end_byte();
                    comments.push((end_byte, instruction.to_string()));
                }
            }
        }
    });

    Ok(comments)
}

/// Visit all nodes in the tree
fn visit_nodes<F>(cursor: &mut TreeCursor, callback: &mut F)
where
    F: FnMut(&Node),
{
    loop {
        let node = cursor.node();
        callback(&node);

        if cursor.goto_first_child() {
            visit_nodes(cursor, callback);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Extract functions that have mantra comments
fn extract_functions_with_mantra(
    root: &Node,
    source: &str,
    file_path: &Path,
    mantra_comments: &mut Vec<(usize, String)>,
    targets: &mut Vec<Target>,
) -> Result<()> {
    let mut cursor = root.walk();

    visit_nodes(&mut cursor, &mut |node| {
        if node.kind() == "function_declaration" || node.kind() == "method_declaration" {
            // Check if there's a mantra comment before this function
            let func_start = node.start_byte();

            // Find the closest mantra comment before this function
            let mut instruction = None;
            mantra_comments.retain(|(comment_end, instr)| {
                if *comment_end < func_start && func_start - comment_end < MAX_COMMENT_GAP {
                    instruction = Some(instr.clone());
                    false // Remove from list once matched
                } else {
                    true
                }
            });

            if let Some(instruction) = instruction {
                if let Ok(target) = parse_function_as_target(node, source, file_path, instruction) {
                    targets.push(target);
                }
            }
        }
    });

    Ok(())
}

/// Parse a function/method declaration as a Target
fn parse_function_as_target(
    node: &Node,
    source: &str,
    _file_path: &Path,
    instruction: String,
) -> Result<Target> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("unknown")
        .to_string();

    // Build signature directly from node
    let signature = extract_signature(node, source);

    Ok(Target {
        name,
        instruction,
        signature,
    })
}

/// Extract signature directly from source
fn extract_signature(node: &Node, source: &str) -> String {
    // Get the text from the start of the function to the opening brace
    let start = node.start_byte();
    let mut end = node.end_byte();

    // Find the body to get just the signature
    if let Some(body) = node.child_by_field_name("body") {
        end = body.start_byte();
    }

    source[start..end]
        .trim()
        .trim_end_matches('{')
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::GoParser;

    #[test]
    fn test_extract_mantra_target() {
        let source = r#"
package main

// mantra: Get user by ID from database
func GetUser(id string) (*User, error) {
    panic("not implemented")
}
"#;

        let mut parser = GoParser::new().unwrap();
        let tree = parser.parse(source).unwrap();
        let file_info = extract_file_info(Path::new("test.go"), source, tree).unwrap();

        assert_eq!(file_info.package_name, "main");
        assert_eq!(file_info.targets.len(), 1);

        let target = &file_info.targets[0];
        assert_eq!(target.name, "GetUser");
        assert_eq!(target.instruction, "Get user by ID from database");
        assert_eq!(target.signature, "func GetUser(id string) (*User, error)");
    }

    #[test]
    fn test_extract_method() {
        let source = r#"
package service

// mantra: Save user to database
func (s *UserService) SaveUser(ctx context.Context, user *User) error {
    panic("not implemented")
}
"#;

        let mut parser = GoParser::new().unwrap();
        let tree = parser.parse(source).unwrap();
        let file_info = extract_file_info(Path::new("test.go"), source, tree).unwrap();

        assert_eq!(file_info.targets.len(), 1);

        let target = &file_info.targets[0];
        assert_eq!(target.name, "SaveUser");
        assert_eq!(
            target.signature,
            "func (s *UserService) SaveUser(ctx context.Context, user *User) error"
        );
    }
}
