use mantra::config::Config;
use mantra::generator::DocumentManager;
use std::path::Path;

#[tokio::test]
async fn test_generate_output() {
    // Enable test mode
    std::env::set_var("MANTRA_TEST_MODE", "1");

    // Create config
    let config = Config {
        url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
        model: "test-model".to_string(),
        api_key: Some("test-key".to_string()),
        log_level: None,
        openrouter: None,
    };

    // Test with simple file
    let file_path = Path::new("examples/simple_test.go");

    // Create document manager
    let mut doc_manager = DocumentManager::new(config, file_path).unwrap();
    let result = doc_manager.generate_all().await.unwrap();

    // Check that line count is reasonable (not too many extra lines)
    let line_count = result.lines().count();

    println!(
        "Generated output ({} lines):\n---\n{}\n---",
        line_count, result
    );

    // Check that mantra comments are preserved
    assert!(result.contains("// mantra:"), "Missing mantra comments");

    // Check that implementations were added
    assert!(
        result.contains("return a + b"),
        "Missing Add implementation"
    );
    assert!(
        result.contains("return n%2 == 0"),
        "Missing IsEven implementation"
    );
    assert!(
        result.contains("return strings.ToUpper(s)"),
        "Missing ToUpper implementation"
    );

    // Original file has 16 lines, allow some extra
    assert!(line_count <= 25, "Too many lines in output: {}", line_count);
}
