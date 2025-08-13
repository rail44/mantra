use mantra::generator::edit_event::{convert_to_lsp_edits, EditEvent};
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

    // Create edit events
    let events = vec![
        EditEvent::new(
            0x12345678,
            "func Add(a, b int) int".to_string(),
            "return a + b".to_string(),
        ),
        EditEvent::new(
            0x87654321,
            "func Multiply(x, y int) int".to_string(),
            "return x * y".to_string(),
        ),
    ];

    // Convert to LSP edits
    let edits = convert_to_lsp_edits(source, &tree, events).unwrap();

    // Check we got edits
    assert_eq!(edits.len(), 2);

    // Check first edit
    let edit1 = &edits[0];
    assert!(edit1.new_text.contains("mantra:checksum:12345678"));
    assert!(edit1.new_text.contains("return a + b"));

    // Check second edit
    let edit2 = &edits[1];
    assert!(edit2.new_text.contains("mantra:checksum:87654321"));
    assert!(edit2.new_text.contains("return x * y"));
}
