use mantra::config::Config;
use mantra::document::DocumentManager;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Create config
    let config = Config {
        url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
        model: "anthropic/claude-3.5-sonnet".to_string(),
        api_key: Some("test-key-123".to_string()),
        log_level: Some("debug".to_string()),
        openrouter: None,
    };

    // Test with simple file
    let file_path = Path::new("examples/simple_test.go");

    println!("Processing file: {}", file_path.display());

    // Mock LLM response for testing
    std::env::set_var("MOCK_LLM_RESPONSE", "return a + b");

    // Create document manager (LSP will be skipped if gopls is not available)
    let mut doc_manager = DocumentManager::new(config, file_path).await?;
    let result = doc_manager.generate_all().await?;

    println!("Generated output:\n{}", result);

    // Check that checksum was added
    if result.contains("mantra:checksum:") {
        println!("✓ Checksum added successfully");
    } else {
        println!("✗ Checksum not found");
    }

    Ok(())
}
