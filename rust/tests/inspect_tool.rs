use anyhow::Result;
use mantra::config::Config;
use mantra::lsp::{Position, Range};
use mantra::tools::{inspect::InspectRequest, InspectTool};
use mantra::workspace::Workspace;
use std::path::PathBuf;

fn create_test_config() -> Config {
    Config {
        url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
        model: "anthropic/claude-3.5-sonnet".to_string(),
        api_key: Some("test-key".to_string()),
        log_level: Some("error".to_string()),
        openrouter: None,
    }
}

#[tokio::test]
async fn test_inspect_tool_scope_management() -> Result<()> {
    let mut tool = InspectTool::new();

    // Test initial scope creation
    let scope_id = tool.create_initial_scope("file:///test.go".to_string(), 5, 15);

    assert_eq!(scope_id, "scope_0");

    // Test scope registration
    let scope2 = tool.register_scope(
        "file:///other.go".to_string(),
        Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 20,
                character: 0,
            },
        },
    );

    assert_eq!(scope2, "scope_1");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires LSP server and actual Go files
async fn test_inspect_tool_with_workspace() -> Result<()> {
    // Create test file with symbol usage and definition
    let test_content = r#"package main

type User struct {
    Name string
    Age  int
}

func GetUser(id string) *User {
    // Use User type here
    return &User{Name: "test", Age: 25}
}"#;

    let test_file = "target/test_inspect.go";
    std::fs::write(test_file, test_content)?;

    // Create workspace
    let config = create_test_config();
    let mut workspace = Workspace::new(PathBuf::from("."), config).await?;

    // Create InspectTool
    let mut tool = InspectTool::new();

    // Register scope for GetUser function (lines 7-10)
    let file_uri = format!(
        "file://{}",
        std::env::current_dir()?.join(test_file).display()
    );
    let scope_id = tool.create_initial_scope(file_uri, 7, 10);

    // Inspect the "User" symbol
    let request = InspectRequest {
        scope_id,
        symbol: "User".to_string(),
    };

    // Create channels for workspace actor
    let (workspace_tx, workspace_rx) = tokio::sync::mpsc::channel(32);

    // Clone for the spawned task
    let workspace_tx_clone = workspace_tx.clone();

    // Spawn workspace actor
    let workspace_handle =
        tokio::spawn(async move { workspace.run_actor(workspace_tx_clone, workspace_rx).await });

    let response = tool.inspect(request, workspace_tx.clone()).await?;

    // Check response
    assert!(response.code.contains("struct"));
    assert!(response.code.contains("Name string"));

    // Clean up
    // Send shutdown command
    workspace_tx
        .send(mantra::workspace::WorkspaceCommand::Shutdown)
        .await?;

    // Wait for workspace to shutdown
    let _ = workspace_handle.await;

    std::fs::remove_file(test_file).ok();

    Ok(())
}
