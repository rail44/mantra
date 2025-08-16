use mantra::config::Config;
use mantra::document::DocumentManager;
use std::path::Path;

fn create_test_config() -> Config {
    Config {
        url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
        model: "anthropic/claude-3.5-sonnet".to_string(),
        api_key: Some("test-key-123".to_string()),
        log_level: Some("error".to_string()),
        openrouter: None,
    }
}

#[tokio::test]
#[ignore] // Requires proper LLM mock setup
async fn test_document_manager_generation() -> Result<(), Box<dyn std::error::Error>> {
    // Create test file
    let test_content = r#"package main

// mantra: Add two numbers
func Add(a, b int) int {
    panic("not implemented")
}"#;

    let test_file = "target/test_document_manager.go";
    std::fs::write(test_file, test_content)?;

    // Create config
    let config = create_test_config();

    // Mock LLM response for testing
    std::env::set_var("MOCK_LLM_RESPONSE", "return a + b");

    // Create document manager
    let mut doc_manager = DocumentManager::new(config, Path::new(test_file)).await?;
    let result = doc_manager.generate_all().await?;

    // Check that checksum was added
    assert!(
        result.contains("mantra:checksum:"),
        "Checksum should be added"
    );
    assert!(
        result.contains("return a + b"),
        "Generated code should be present"
    );

    // Clean up
    std::fs::remove_file(test_file).ok();

    Ok(())
}

#[tokio::test]
async fn test_document_manager_actor_commands() -> Result<(), Box<dyn std::error::Error>> {
    use mantra::document::DocumentCommand;
    use tokio::sync::{mpsc, oneshot};

    // Create test file
    let test_content = "package main\n\nfunc main() {}\n";
    let test_file = "target/test_document_actor.go";
    std::fs::write(test_file, test_content)?;

    let config = create_test_config();

    // Create document manager and run as actor
    let mut doc_manager = DocumentManager::new(config, Path::new(test_file)).await?;

    let (tx, rx) = mpsc::channel(32);

    // Spawn actor
    let actor_handle = tokio::spawn(async move { doc_manager.run_actor(rx).await });

    // Test GetSource command
    let (response_tx, response_rx) = oneshot::channel();
    tx.send(DocumentCommand::GetSource {
        response: response_tx,
    })
    .await?;

    let source = response_rx.await??;
    assert_eq!(source, test_content);

    // Shutdown actor
    tx.send(DocumentCommand::Shutdown).await?;
    actor_handle.await??;

    // Clean up
    std::fs::remove_file(test_file).ok();

    Ok(())
}
