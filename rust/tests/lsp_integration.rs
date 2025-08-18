use anyhow::Result;
use mantra::core::types::{Position, TextDocumentIdentifier};
use mantra::lsp::{Client as LspClient, TextDocumentItem};

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
            uri: "file:///tmp/test.go".to_string(),
            language_id: "go".to_string(),
            version: 1,
            text: test_content.to_string(),
        })
        .await?;

    // Test hover
    let hover_result = client
        .hover(
            TextDocumentIdentifier {
                uri: "file:///tmp/test.go".to_string(),
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

/// Test LSP definition functionality
#[tokio::test]
#[ignore] // Run with --ignored flag when gopls is available
async fn test_lsp_definition() -> Result<()> {
    let client = LspClient::new("gopls", &[]).await?;

    // Initialize with definition capability
    let capabilities = LspClient::default_capabilities();
    let workspace_folders = LspClient::default_workspace_folders("file:///tmp")?;

    client
        .initialize(
            Some(std::process::id()),
            Some("file:///tmp".to_string()),
            capabilities,
            Some(workspace_folders),
        )
        .await?;
    client.initialized().await?;

    // Open a document with a function call
    let test_content = r#"package main

func helper() string {
    return "test"
}

func main() {
    result := helper()
    println(result)
}"#;

    client
        .did_open(TextDocumentItem {
            uri: "file:///tmp/test_def.go".to_string(),
            language_id: "go".to_string(),
            version: 1,
            text: test_content.to_string(),
        })
        .await?;

    // Test definition - should find the helper function
    let definition = client
        .definition(
            TextDocumentIdentifier {
                uri: "file:///tmp/test_def.go".to_string(),
            },
            Position {
                line: 7,
                character: 14,
            }, // Position on "helper" call
        )
        .await?;

    if let Some(loc) = definition {
        assert_eq!(
            loc.range.start.line, 2,
            "Should find function definition at line 2"
        );
    }

    // Shutdown
    client.shutdown().await?;

    Ok(())
}
