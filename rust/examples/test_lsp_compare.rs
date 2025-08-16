use anyhow::Result;
use mantra::lsp::{Client as LspClient, Position, TextDocumentIdentifier, TextDocumentItem};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Test source with various scenarios
    let test_source = r#"package main

import "context"

// User represents a user
type User struct {
    ID   string
    Name string
}

// UserService handles operations
type UserService struct {
    db Database
}

// Database interface (undefined - to test behavior)
type Database interface {
    Save(ctx context.Context, data interface{}) error
}

// SaveUser saves a user
func (s *UserService) SaveUser(ctx context.Context, user *User) error {
    return s.db.Save(ctx, user)
}

// ProcessData processes generic data
func ProcessData(data interface{}) error {
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
                    },
                    "definition": {},
                    "typeDefinition": {}
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

    println!("=== Comparing LSP RPC Methods ===\n");

    // Test Case 1: On "*User" in SaveUser parameter
    println!("TEST 1: On '*User' type in SaveUser parameter");
    println!("Code: func (s *UserService) SaveUser(ctx context.Context, user *User) error");
    // Line 21 (0-indexed): func (s *UserService) SaveUser(ctx context.Context, user *User) error
    //                                                                      ^User starts at 59
    let pos = Position {
        line: 21,
        character: 59,
    }; // Position on "User"

    compare_methods(&client, &doc_id, pos).await?;

    // Test Case 2: On "context.Context"
    println!("\n---\nTEST 2: On 'context.Context' imported type");
    println!("Code: func (s *UserService) SaveUser(ctx context.Context, user *User) error");
    // Line 21 (0-indexed): func (s *UserService) SaveUser(ctx context.Context, user *User) error
    //                                                     ^Context starts at 45
    let pos = Position {
        line: 21,
        character: 45,
    }; // Position on "Context"

    compare_methods(&client, &doc_id, pos).await?;

    // Test Case 3: On "interface{}"
    println!("\n---\nTEST 3: On 'interface{{}}' in ProcessData");
    println!("Code: func ProcessData(data interface{{}}) error");
    // Line 26 (0-indexed): func ProcessData(data interface{}) error
    //                                       ^interface starts at 22
    let pos = Position {
        line: 26,
        character: 22,
    }; // Position on "interface"

    compare_methods(&client, &doc_id, pos).await?;

    // Test Case 4: On method call "s.db.Save"
    println!("\n---\nTEST 4: On method call 's.db.Save'");
    println!("Code: return s.db.Save(ctx, user)");
    // Line 22 (0-indexed): return s.db.Save(ctx, user)
    //                              ^Save starts at 13
    let pos = Position {
        line: 22,
        character: 13,
    }; // Position on "Save"

    compare_methods(&client, &doc_id, pos).await?;

    client.shutdown().await?;
    Ok(())
}

async fn compare_methods(
    client: &LspClient,
    doc_id: &TextDocumentIdentifier,
    pos: Position,
) -> Result<()> {
    // 1. Hover
    print!("  hover: ");
    match client.hover(doc_id.clone(), pos).await? {
        Some(hover) => {
            let content = match hover.contents {
                mantra::lsp::MarkupContent::Markdown { value, .. } => value,
                mantra::lsp::MarkupContent::PlainText(text) => text,
            };
            // Print first line or summary
            let first_line = content.lines().next().unwrap_or("");
            println!(
                "{}",
                if first_line.len() > 60 {
                    format!("{}...", &first_line[..60])
                } else {
                    first_line.to_string()
                }
            );
        }
        None => println!("No hover info"),
    }

    // 2. Definition
    print!("  definition: ");
    match client.definition(doc_id.clone(), pos).await? {
        Some(location) => {
            let file = location.uri.rsplit('/').next().unwrap_or(&location.uri);
            println!(
                "{}:{}:{}",
                file,
                location.range.start.line + 1,
                location.range.start.character
            );
        }
        None => println!("No definition"),
    }

    // 3. Type Definition
    print!("  typeDefinition: ");
    match client.type_definition(doc_id.clone(), pos).await? {
        Some(location) => {
            let file = location.uri.rsplit('/').next().unwrap_or(&location.uri);
            println!(
                "{}:{}:{}",
                file,
                location.range.start.line + 1,
                location.range.start.character
            );
        }
        None => println!("No type definition"),
    }

    Ok(())
}
