use anyhow::Result;
use lsp_types::{Position, TextDocumentIdentifier, TextDocumentItem, Uri};
use mantra::lsp::Client as LspClient;

/// Test basic LSP client functionality
/// Requires gopls to be installed
#[tokio::test]
#[ignore] // Run with --ignored flag when gopls is available
async fn test_lsp_basic_operations() -> Result<()> {
    let client = LspClient::new("gopls", &[]).await?;

    // Initialize
    let capabilities = LspClient::default_capabilities();
    let workspace_folders = LspClient::default_workspace_folders("file:///tmp")?;

    let _init_result = client
        .initialize(
            Some(std::process::id()),
            Some("file:///tmp".to_string()),
            capabilities,
            Some(workspace_folders),
        )
        .await?;

    // InitializeResult is returned successfully
    client.initialized().await?;

    // Open a document
    let test_content = r#"package main

import "fmt"

func main() {
    fmt.Println("Hello, World!")
}"#;

    client
        .did_open(TextDocumentItem {
            uri: "file:///tmp/test.go".parse::<Uri>().unwrap(),
            language_id: "go".to_string(),
            version: 1,
            text: test_content.to_string(),
        })
        .await?;

    // Test hover
    let hover_result = client
        .hover(
            TextDocumentIdentifier {
                uri: "file:///tmp/test.go".parse::<Uri>().unwrap(),
            },
            Position {
                line: 5,
                character: 8,
            }, // Position on "Println"
        )
        .await?;

    assert!(hover_result.is_some(), "Hover should return information");

    // Shutdown
    client.shutdown().await?;

    Ok(())
}

// Test for definition functionality removed - definition method was removed as unused
