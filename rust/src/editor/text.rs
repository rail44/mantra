/// Simple text editor that manages string content and applies edits
pub struct IncrementalEditor {
    /// Current source code
    source: String,
}

impl IncrementalEditor {
    /// Create a new editor with initial source
    pub fn new(source: String) -> Self {
        Self { source }
    }

    /// Get current source
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Apply a simple text edit
    pub fn apply_edit(&mut self, start_byte: usize, end_byte: usize, new_text: String) {
        let before = &self.source[..start_byte];
        let after = &self.source[end_byte..];
        self.source = format!("{}{}{}", before, new_text, after);
    }

    /// Insert text at a specific position
    pub fn insert(&mut self, position: usize, text: String) {
        self.apply_edit(position, position, text);
    }

    /// Replace text in a range
    pub fn replace(&mut self, start: usize, end: usize, text: String) {
        self.apply_edit(start, end, text);
    }

    /// Convert byte position to line/column
    pub fn byte_to_line_col(&self, byte_pos: usize) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;
        let mut current_byte = 0;

        for ch in self.source.chars() {
            if current_byte >= byte_pos {
                break;
            }

            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += ch.len_utf8();
            }

            current_byte += ch.len_utf8();
        }

        (line, col)
    }

    /// Convert line/column to byte position
    pub fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        let mut current_line = 0;
        let mut byte_offset = 0;
        let mut line_start_byte = 0;

        for ch in self.source.chars() {
            if current_line == line {
                // We're at the target line
                if (byte_offset - line_start_byte) >= col {
                    return byte_offset;
                }
            }

            if ch == '\n' {
                current_line += 1;
                line_start_byte = byte_offset + ch.len_utf8();
            }

            byte_offset += ch.len_utf8();

            if current_line > line {
                break;
            }
        }

        // Return the position at the target line and column
        line_start_byte + col
    }
}

/// Indent code with given prefix
pub fn indent_code(code: &str, indent: &str) -> String {
    let code = code.trim();
    let code = if code.starts_with('{') && code.ends_with('}') {
        &code[1..code.len() - 1]
    } else {
        code
    };

    code.lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{}{}", indent, line.trim_start())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_edit() {
        let mut editor = IncrementalEditor::new("Hello, world!".to_string());

        // Replace "world" with "Rust"
        editor.replace(7, 12, "Rust".to_string());
        assert_eq!(editor.source(), "Hello, Rust!");

        // Insert text
        editor.insert(7, "beautiful ".to_string());
        assert_eq!(editor.source(), "Hello, beautiful Rust!");
    }

    #[test]
    fn test_position_conversion() {
        let editor = IncrementalEditor::new("line1\nline2\nline3".to_string());

        // Test byte to line/col
        assert_eq!(editor.byte_to_line_col(0), (0, 0)); // Start of line1
        assert_eq!(editor.byte_to_line_col(6), (1, 0)); // Start of line2
        assert_eq!(editor.byte_to_line_col(12), (2, 0)); // Start of line3

        // Test line/col to byte
        assert_eq!(editor.line_col_to_byte(0, 0), 0); // Start of line1
        assert_eq!(editor.line_col_to_byte(1, 0), 6); // Start of line2
        assert_eq!(editor.line_col_to_byte(2, 0), 12); // Start of line3
    }
}
