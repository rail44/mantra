use mantra::generation::{convert_to_lsp_edits, EditEvent};
use mantra::parser::{checksum::calculate_checksum, target::Target, GoParser};

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

    // Calculate actual checksums for debugging
    let add_target = Target {
        name: "Add".to_string(),
        instruction: "Add two numbers".to_string(),
        signature: "func Add(a, b int) int".to_string(),
    };
    let add_checksum = calculate_checksum(&add_target);

    let multiply_target = Target {
        name: "Multiply".to_string(),
        instruction: "Multiply two numbers".to_string(),
        signature: "func Multiply(x, y int) int".to_string(),
    };
    let multiply_checksum = calculate_checksum(&multiply_target);

    eprintln!("Actual Add checksum: 0x{:x}", add_checksum);
    eprintln!("Actual Multiply checksum: 0x{:x}", multiply_checksum);

    // Create edit events with correct checksums
    let events = vec![
        EditEvent::new(
            add_checksum,
            "func Add(a, b int) int".to_string(),
            "return a + b".to_string(),
        ),
        EditEvent::new(
            multiply_checksum,
            "func Multiply(x, y int) int".to_string(),
            "return x * y".to_string(),
        ),
    ];

    // Convert to LSP edits
    let edits = convert_to_lsp_edits(source, &tree, events).unwrap();

    // Debug output for edits
    eprintln!("Number of edits: {}", edits.len());
    for (i, edit) in edits.iter().enumerate() {
        eprintln!("Edit {}: {:?}", i, edit.new_text);
    }

    // Check we got edits (2 for each function: checksum comment + body)
    assert_eq!(
        edits.len(),
        4,
        "Expected 4 edits (2 per function), got {}",
        edits.len()
    );

    // Find edits containing our expected content
    let has_add_checksum = edits.iter().any(|e| {
        e.new_text
            .contains(&format!("mantra:checksum:{:x}", add_checksum))
    });
    let has_add_body = edits.iter().any(|e| e.new_text.contains("return a + b"));
    let has_multiply_checksum = edits.iter().any(|e| {
        e.new_text
            .contains(&format!("mantra:checksum:{:x}", multiply_checksum))
    });
    let has_multiply_body = edits.iter().any(|e| e.new_text.contains("return x * y"));

    assert!(has_add_checksum, "Should have Add checksum edit");
    assert!(has_add_body, "Should have Add body edit");
    assert!(has_multiply_checksum, "Should have Multiply checksum edit");
    assert!(has_multiply_body, "Should have Multiply body edit");
}
