/// Find the start position of checksum comments before a function.
///
/// Walks backwards from `func_start` to find all consecutive `// mantra:checksum:` lines.
/// Returns the byte position where the checksum comment region starts, or `func_start`
/// if no checksum comments are found.
pub fn find_checksum_region_start(text: &str, func_start: usize) -> usize {
    let before_func = &text[..func_start];
    let trimmed = before_func.trim_end();
    if trimmed.is_empty() {
        return func_start;
    }

    let mut checksum_region_start: Option<usize> = None;
    let mut current_pos = trimmed.len();

    loop {
        let line_start = trimmed[..current_pos].rfind('\n').map_or(0, |i| i + 1);
        let line = trimmed[line_start..current_pos].trim();

        if line.starts_with("// mantra:checksum:") {
            checksum_region_start = Some(line_start);

            if line_start == 0 {
                break;
            }
            current_pos = line_start - 1;
        } else {
            break;
        }
    }

    checksum_region_start.unwrap_or(func_start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_checksum_comment() {
        let text = "package main\n\nfunc Foo() {}";
        let func_start = text.find("func").unwrap();
        assert_eq!(find_checksum_region_start(text, func_start), func_start);
    }

    #[test]
    fn test_single_checksum_comment() {
        let text = "package main\n\n// mantra:checksum:abc123\nfunc Foo() {}";
        let func_start = text.find("func").unwrap();
        let expected = text.find("// mantra:checksum").unwrap();
        assert_eq!(find_checksum_region_start(text, func_start), expected);
    }

    #[test]
    fn test_multiple_checksum_comments() {
        let text =
            "package main\n\n// mantra:checksum:abc\n// mantra:checksum:def\nfunc Foo() {}";
        let func_start = text.find("func").unwrap();
        let expected = text.find("// mantra:checksum:abc").unwrap();
        assert_eq!(find_checksum_region_start(text, func_start), expected);
    }

    #[test]
    fn test_instruction_comment_not_included() {
        let text = "package main\n\n// mantra: instruction\n// mantra:checksum:abc\nfunc Foo() {}";
        let func_start = text.find("func").unwrap();
        let expected = text.find("// mantra:checksum").unwrap();
        assert_eq!(find_checksum_region_start(text, func_start), expected);
    }
}
