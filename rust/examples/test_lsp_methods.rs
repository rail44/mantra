use anyhow::Result;
use mantra::lsp::{Client as LspClient, Position, TextDocumentIdentifier, TextDocumentItem};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Test Go source with various types
    let test_source = r#"package main

import "context"

// User represents a user in the system
type User struct {
    ID   string
    Name string
    Age  int
}

// UserService handles user operations
type UserService struct {
    db Database
}

// SaveUser saves a user to the database
func (s *UserService) SaveUser(ctx context.Context, user *User) error {
    return s.db.Save(ctx, user)
}

// GetUser retrieves a user by ID
func GetUser(id string) (*User, error) {
    return nil, nil
}
"#;

    // Start gopls
    let client = LspClient::new("gopls", &[]).await?;

    // Initialize
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

    // Open document
    let uri = format!("{}/test.go", root_uri);
    client
        .did_open(TextDocumentItem {
            uri: uri.clone(),
            language_id: "go".to_string(),
            version: 1,
            text: test_source.to_string(),
        })
        .await?;

    // Wait a bit for gopls to process
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n=== Testing LSP RPC Methods ===\n");

    // Test positions (0-indexed lines):
    // Line 15 (0-indexed): "func (s *UserService) SaveUser(ctx context.Context, user *User) error {"
    let user_param_pos = Position {
        line: 15,
        character: 60,
    }; // on "User"

    // Line 15: "ctx context.Context" - parameter type
    let ctx_param_pos = Position {
        line: 15,
        character: 40,
    }; // on "Context"

    // Line 11 (0-indexed): "    db Database" - field type
    let db_field_pos = Position {
        line: 11,
        character: 7,
    }; // on "Database"

    let doc_id = TextDocumentIdentifier { uri: uri.clone() };

    // 1. Test hover
    println!("1. HOVER on 'User' parameter:");
    if let Some(hover) = client.hover(doc_id.clone(), user_param_pos).await? {
        match hover.contents {
            mantra::lsp::MarkupContent::Markdown { value, .. } => {
                println!("   {}", value);
            }
            mantra::lsp::MarkupContent::PlainText(text) => {
                println!("   {}", text);
            }
        }
    } else {
        println!("   No hover info");
    }

    println!("\n2. HOVER on 'Context' type:");
    if let Some(hover) = client.hover(doc_id.clone(), ctx_param_pos).await? {
        match hover.contents {
            mantra::lsp::MarkupContent::Markdown { value, .. } => {
                println!("   {}", value);
            }
            mantra::lsp::MarkupContent::PlainText(text) => {
                println!("   {}", text);
            }
        }
    } else {
        println!("   No hover info");
    }

    println!("\n3. HOVER on undefined 'Database' type:");
    if let Some(hover) = client.hover(doc_id.clone(), db_field_pos).await? {
        match hover.contents {
            mantra::lsp::MarkupContent::Markdown { value, .. } => {
                println!("   {}", value);
            }
            mantra::lsp::MarkupContent::PlainText(text) => {
                println!("   {}", text);
            }
        }
    } else {
        println!("   No hover info");
    }

    // Note: To test definition, typeDefinition, etc., we need to implement those methods first
    println!("\n=== Note ===");
    println!("To test other RPC methods (definition, typeDefinition, references),");
    println!("we need to implement them in the LSP client first.");

    client.shutdown().await?;
    Ok(())
}
