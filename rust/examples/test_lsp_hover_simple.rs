use anyhow::Result;
use mantra::lsp::{Client as LspClient, Position, TextDocumentIdentifier, TextDocumentItem};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Simpler test with exact positions
    let test_source = r#"package main

type User struct {
    ID string
}

func SaveUser(user *User) error {
    return nil
}"#;

    // Start gopls
    let client = LspClient::new("gopls", &[]).await?;

    let root_uri = format!("file://{}", std::env::current_dir()?.display());
    client
        .initialize(
            Some(std::process::id()),
            Some(root_uri.clone()),
            json!({
                "textDocument": {
                    "hover": {
                        "contentFormat": ["markdown", "plaintext"]
                    }
                }
            }),
            None,
        )
        .await?;
    client.initialized().await?;

    let uri = format!("{}/test.go", root_uri);
    client
        .did_open(TextDocumentItem {
            uri: uri.clone(),
            language_id: "go".to_string(),
            version: 1,
            text: test_source.to_string(),
        })
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let doc_id = TextDocumentIdentifier { uri: uri.clone() };

    // Line 6 (0-indexed): "func SaveUser(user *User) error {"
    // Position on "User" in "*User"
    // "func SaveUser(user *User) error {"
    //  0         1         2
    //  0123456789012345678901234
    //                      ^User starts at 20

    println!("Testing hover on User type in parameter:");
    let result = client
        .hover(
            doc_id.clone(),
            Position {
                line: 6,
                character: 20,
            },
        )
        .await?;

    if let Some(hover) = result {
        match hover.contents {
            mantra::lsp::MarkupContent::Markdown { value, .. } => {
                println!("Markdown response:\n{}", value);
            }
            mantra::lsp::MarkupContent::PlainText(text) => {
                println!("Plain text response:\n{}", text);
            }
        }
    } else {
        println!("No hover information");
    }

    // Also test on the struct definition itself
    println!("\n---\nTesting hover on User struct definition:");
    let result = client
        .hover(
            doc_id.clone(),
            Position {
                line: 2,
                character: 5,
            },
        )
        .await?;

    if let Some(hover) = result {
        match hover.contents {
            mantra::lsp::MarkupContent::Markdown { value, .. } => {
                println!("Markdown response:\n{}", value);
            }
            mantra::lsp::MarkupContent::PlainText(text) => {
                println!("Plain text response:\n{}", text);
            }
        }
    } else {
        println!("No hover information");
    }

    client.shutdown().await?;
    Ok(())
}
