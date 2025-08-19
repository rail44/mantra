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
    fn test_indent_code() {
        // Test basic indentation
        let code = "line1\nline2\nline3";
        let indented = indent_code(code, "    ");
        assert_eq!(indented, "    line1\n    line2\n    line3");

        // Test with braces
        let code = "{\n    return true\n}";
        let indented = indent_code(code, "  ");
        assert_eq!(indented, "\n  return true");

        // Test with empty lines
        let code = "line1\n\nline2";
        let indented = indent_code(code, "\t");
        assert_eq!(indented, "\tline1\n\n\tline2");

        // Test with already indented code
        let code = "  already\n    indented";
        let indented = indent_code(code, ">>>");
        assert_eq!(indented, ">>>already\n>>>indented");
    }
}
