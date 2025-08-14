use anyhow::Result;

// Re-use LSP types from editor module
pub use crate::editor::{Position, Range, TextEdit};

/// Edit event that describes a change to apply to the source
/// Uses mantra checksum as a stable identifier
#[derive(Debug, Clone)]
pub struct EditEvent {
    /// Checksum of the target (stable identifier)
    pub checksum: u64,

    /// Function signature for finding the node in tree-sitter
    pub signature: String,

    /// New body content to replace
    pub new_body: String,
}

impl EditEvent {
    pub fn new(checksum: u64, signature: String, new_body: String) -> Self {
        Self {
            checksum,
            signature,
            new_body,
        }
    }
}

/// Converts edit events to LSP-style text edits
pub fn convert_to_lsp_edits(
    source: &str,
    tree: &tree_sitter::Tree,
    events: Vec<EditEvent>,
) -> Result<Vec<TextEdit>> {
    use crate::parser::editor::find_function_info;

    let mut edits = Vec::new();

    for event in events {
        // Find function in tree by signature
        if let Some(func_info) = find_function_info(source, tree, &event.signature) {
            // We need to replace two parts:
            // 1. Add/update checksum comment before function
            // 2. Replace function body

            // Find the line before the function to add checksum comment
            let func_start_pos = byte_to_position_single(source, func_info.start_byte);
            let body_start_pos = byte_to_position_single(source, func_info.body_start);
            let body_end_pos = byte_to_position_single(source, func_info.body_end);

            // Check if there's already a checksum comment
            let lines: Vec<&str> = source.lines().collect();
            let has_checksum = func_start_pos.line > 0
                && lines
                    .get((func_start_pos.line - 1) as usize)
                    .map(|line| line.contains("// mantra:checksum:"))
                    .unwrap_or(false);

            // Add checksum comment if not present
            if !has_checksum {
                // Insert checksum comment before the function
                let checksum_comment = format!("// mantra:checksum:{:x}\n", event.checksum);
                edits.push(TextEdit::new(
                    Range::new(
                        Position::new(func_start_pos.line, 0),
                        Position::new(func_start_pos.line, 0),
                    ),
                    checksum_comment,
                ));
            } else {
                // Update existing checksum comment
                let checksum_line = func_start_pos.line - 1;
                let checksum_comment = format!("// mantra:checksum:{:x}", event.checksum);
                edits.push(TextEdit::new(
                    Range::new(
                        Position::new(checksum_line, 0),
                        Position::new(checksum_line, lines[checksum_line as usize].len() as u32),
                    ),
                    checksum_comment,
                ));
            }

            // Replace function body
            let indented_body = indent_code(&event.new_body, "\t");
            let formatted_body = if indented_body.trim().is_empty() {
                "{\n\tpanic(\"not implemented\")\n}".to_string()
            } else {
                format!("{{\n{}\n}}", indented_body)
            };

            edits.push(TextEdit::new(
                Range::new(body_start_pos, body_end_pos),
                formatted_body,
            ));
        }
    }

    Ok(edits)
}

/// Convert single byte position to line/character position
fn byte_to_position_single(source: &str, byte_pos: usize) -> Position {
    let mut line = 0;
    let mut line_start_byte = 0;
    let mut current_byte = 0;

    for ch in source.chars() {
        if current_byte >= byte_pos {
            return Position::new(line as u32, (byte_pos - line_start_byte) as u32);
        }

        if ch == '\n' {
            line += 1;
            line_start_byte = current_byte + ch.len_utf8();
        }
        current_byte += ch.len_utf8();
    }

    Position::new(line as u32, (byte_pos - line_start_byte) as u32)
}

/// Indent code with given prefix
fn indent_code(code: &str, indent: &str) -> String {
    crate::incremental_editor::indent_code(code, indent)
}
