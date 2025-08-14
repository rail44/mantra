use mantra::generation::{convert_to_lsp_edits, EditEvent};
use mantra::parser::GoParser;

#[test]
fn test_edit_event_to_lsp() {
    let source = r#"package main

// mantra: Add two numbers
func Add(a, b int) int {
    panic("not implemented")
}

// mantra: Multiply two numbers  
func Multiply(x, y int) int {
    panic("not implemented")
}"#;

    let mut parser = GoParser::new().unwrap();
    let tree = parser.parse(source).unwrap();

    // Create edit events with correct checksums
    let events = vec![
        EditEvent::new(
            0x69a25a3ff4512c52,
            "func Add(a, b int) int".to_string(),
            "return a + b".to_string(),
        ),
        EditEvent::new(
            0x982fdc146e597129,
            "func Multiply(x, y int) int".to_string(),
            "return x * y".to_string(),
        ),
    ];

    // Convert to LSP edits
    let edits = convert_to_lsp_edits(source, &tree, events).unwrap();

    // Check we got edits (2 for each function: checksum comment + body)
    assert_eq!(edits.len(), 4);

    // Find edits containing our expected content
    let has_add_checksum = edits
        .iter()
        .any(|e| e.new_text.contains("mantra:checksum:69a25a3ff4512c52"));
    let has_add_body = edits.iter().any(|e| e.new_text.contains("return a + b"));
    let has_multiply_checksum = edits
        .iter()
        .any(|e| e.new_text.contains("mantra:checksum:982fdc146e597129"));
    let has_multiply_body = edits.iter().any(|e| e.new_text.contains("return x * y"));

    assert!(has_add_checksum, "Should have Add checksum edit");
    assert!(has_add_body, "Should have Add body edit");
    assert!(has_multiply_checksum, "Should have Multiply checksum edit");
    assert!(has_multiply_body, "Should have Multiply body edit");
}
