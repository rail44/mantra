use mantra::config::Config;
use mantra::generator::Generator;
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

    // Create generator
    let generator = Generator::new(config).unwrap();

    // Test with simple file
    let file_path = Path::new("examples/simple_test.go");
    let result = generator.generate_file(file_path).await.unwrap();

    // Check that all functions have checksums
    assert!(result.contains("// mantra:checksum:"));

    // Check that implementations were added
    assert!(result.contains("return a + b"));
    assert!(result.contains("return n%2 == 0"));
    assert!(result.contains("return strings.ToUpper(s)"));

    // Check that line count is reasonable (not too many extra lines)
    let line_count = result.lines().count();

    println!(
        "Generated output ({} lines):\n---\n{}\n---",
        line_count, result
    );

    // Original file has 16 lines, allow some extra
    assert!(line_count <= 25, "Too many lines in output: {}", line_count);
}
