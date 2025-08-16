use mantra::config::Config;
use mantra::document::DocumentCommand;
use mantra::workspace::{Workspace, WorkspaceCommand};
use std::path::{Path, PathBuf};
use tokio::sync::oneshot;

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

    // Create workspace and get document
    let workspace_tx = Workspace::spawn(PathBuf::from("target"), config).await?;

    let file_path = Path::new(test_file);
    let (tx, rx) = oneshot::channel();
    workspace_tx
        .send(WorkspaceCommand::GenerateFile {
            file_path: file_path.to_path_buf(),
            response: tx,
        })
        .await?;

    let result = rx.await??;

    // Check that checksum was added
    assert!(
        result.contains("mantra:checksum:"),
        "Checksum should be added"
    );
    assert!(
        result.contains("return a + b"),
        "Generated code should be present"
    );

    // Shutdown workspace
    workspace_tx.send(WorkspaceCommand::Shutdown).await?;

    // Clean up
    std::fs::remove_file(test_file).ok();

    Ok(())
}

#[tokio::test]
async fn test_document_manager_actor_commands() -> Result<(), Box<dyn std::error::Error>> {
    // Create test file
    let test_content = "package main\n\nfunc main() {}\n";
    let test_file = "target/test_document_actor.go";
    std::fs::write(test_file, test_content)?;

    let config = create_test_config();

    // Create workspace and get document
    let workspace_tx = Workspace::spawn(PathBuf::from("target"), config).await?;

    let file_uri = format!(
        "file://{}",
        std::env::current_dir()?.join(test_file).display()
    );
    let (tx, rx) = oneshot::channel();
    workspace_tx
        .send(WorkspaceCommand::GetDocument {
            uri: file_uri,
            response: tx,
        })
        .await?;

    let document_tx = rx.await??;

    // Test GetSource command
    let (response_tx, response_rx) = oneshot::channel();
    document_tx
        .send(DocumentCommand::GetSource {
            response: response_tx,
        })
        .await?;

    let source = response_rx.await??;
    assert_eq!(source, test_content);

    // Shutdown workspace
    workspace_tx.send(WorkspaceCommand::Shutdown).await?;

    // Clean up
    std::fs::remove_file(test_file).ok();

    Ok(())
}
