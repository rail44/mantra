use anyhow::Result;
use lsp_types::{FormattingOptions, TextDocumentIdentifier};
use mantra::lsp::Client as LspClient;

#[tokio::test]
async fn test_format_document() -> Result<()> {
    // Start LSP client
    let client = LspClient::new("gopls", &[]).await?;

    // Initialize with workspace
    let workspace_uri = "file:///tmp/test_workspace";
    let capabilities = LspClient::default_capabilities();
    let workspace_folders = LspClient::default_workspace_folders(workspace_uri)?;

    client
        .initialize(
            Some(std::process::id()),
            Some(workspace_uri.to_string()),
            capabilities,
            Some(workspace_folders),
        )
        .await?;

    client.initialized().await?;

    // Create a test Go file with unformatted code
    let test_code = r#"package main
import "fmt"
func main(){
fmt.Println("Hello, World!")
}"#;

    // Create temp file
    let temp_file = tempfile::NamedTempFile::new()?;
    let temp_path = temp_file.path().to_path_buf();
    let temp_uri = format!("file://{}", temp_path.display());

    // Write test code
    tokio::fs::write(&temp_path, test_code).await?;

    // Open document in LSP
    let text_document_item = lsp_types::TextDocumentItem {
        uri: temp_uri.parse()?,
        language_id: "go".to_string(),
        version: 1,
        text: test_code.to_string(),
    };

    client.did_open(text_document_item).await?;

    // Request formatting
    let text_document = TextDocumentIdentifier {
        uri: temp_uri.parse()?,
    };

    let formatting_options = FormattingOptions {
        tab_size: 4,
        insert_spaces: false,
        trim_trailing_whitespace: Some(true),
        insert_final_newline: Some(true),
        trim_final_newlines: Some(true),
        ..Default::default()
    };

    let result = client
        .format_document(text_document, formatting_options)
        .await?;

    // Check that we got formatting edits
    assert!(result.is_some(), "Expected formatting edits");

    if let Some(edits) = result {
        assert!(!edits.is_empty(), "Expected non-empty edits");

        // The formatted code should have proper indentation
        for edit in &edits {
            println!("Edit: {:?}", edit);
        }
    }

    // Shutdown LSP
    client.shutdown().await?;

    Ok(())
}
