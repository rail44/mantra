use anyhow::Result;
use mantra::config::Config;
use mantra::document::DocumentCommand;
use mantra::lsp::{Position, Range};
use mantra::tools::{inspect::InspectRequest, InspectTool};
use mantra::workspace::Workspace;
use std::path::PathBuf;
use tokio::sync::oneshot;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Create test Go file
    let test_content = r#"package main

import "fmt"

type User struct {
    ID   string
    Name string
    Age  int
}

func GetUser(id string) (*User, error) {
    // This function uses the User type
    user := &User{
        ID:   id,
        Name: "John Doe",
        Age:  30,
    }
    return user, nil
}

func main() {
    user, err := GetUser("123")
    if err != nil {
        fmt.Println("Error:", err)
        return
    }
    fmt.Printf("User: %+v\n", user)
}"#;

    let test_file = "target/demo_inspect.go";
    std::fs::write(test_file, test_content)?;
    println!("Created test file: {}", test_file);

    // Create config and workspace
    let config = Config {
        url: "https://test.com".to_string(),
        model: "test".to_string(),
        api_key: Some("test".to_string()),
        log_level: Some("info".to_string()),
        openrouter: None,
    };

    let mut workspace = Workspace::new(PathBuf::from("."), config).await?;
    println!("Workspace initialized");

    // Get document actor
    let file_uri = format!(
        "file://{}",
        std::env::current_dir()?.join(test_file).display()
    );
    let document = workspace.get_document(&file_uri).await?;
    println!("Document actor created for: {}", file_uri);

    // Test 1: Get source to verify document is loaded
    println!("\n=== Test 1: Verify document loaded ===");
    let (tx, rx) = oneshot::channel();
    document
        .send(DocumentCommand::GetSource { response: tx })
        .await?;
    let source = rx.await??;
    println!("Document loaded, size: {} bytes", source.len());

    // Test 2: Find symbol in document
    println!("\n=== Test 2: Find symbol 'User' in GetUser function ===");
    let (tx, rx) = oneshot::channel();
    document
        .send(DocumentCommand::FindSymbol {
            range: Range {
                start: Position {
                    line: 10,
                    character: 0,
                }, // GetUser function starts
                end: Position {
                    line: 18,
                    character: 0,
                }, // GetUser function ends
            },
            symbol: "User".to_string(),
            response: tx,
        })
        .await?;

    match rx.await? {
        Ok(position) => {
            println!(
                "Found 'User' at line {}, column {}",
                position.line, position.character
            );
        }
        Err(e) => {
            println!("Failed to find symbol: {}", e);
        }
    }

    // Test 3: Use InspectTool
    println!("\n=== Test 3: InspectTool test ===");
    let mut inspect_tool = InspectTool::new();

    // Register scope for GetUser function
    let scope_id = inspect_tool.create_initial_scope(file_uri.clone(), 10, 18);
    println!("Created scope '{}' for GetUser function", scope_id);

    // Try to inspect the User type
    let request = InspectRequest {
        scope_id: scope_id.clone(),
        symbol: "User".to_string(),
    };

    println!("Attempting to inspect 'User' symbol...");

    // First, let's check what LSP returns directly
    println!("\n--- Debug: LSP definition check ---");
    let lsp_client = workspace.lsp_client();
    let symbol_pos = Position {
        line: 10,
        character: 26,
    }; // Where we found 'User'

    if let Ok(Some(definition)) = lsp_client
        .definition(
            mantra::lsp::TextDocumentIdentifier {
                uri: file_uri.clone(),
            },
            symbol_pos,
        )
        .await
    {
        println!("LSP returned definition location:");
        println!("  URI: {}", definition.uri);
        println!(
            "  Range: line {} col {} to line {} col {}",
            definition.range.start.line,
            definition.range.start.character,
            definition.range.end.line,
            definition.range.end.character,
        );

        // Get the text at that range to see what it covers
        let (tx, rx) = oneshot::channel();
        document
            .send(DocumentCommand::GetText {
                range: definition.range.clone(),
                response: tx,
            })
            .await?;

        if let Ok(text) = rx.await? {
            println!("  Text at range: '{}'", text);
        }
    }
    println!("--- End debug ---\n");

    match inspect_tool.inspect(request, &mut workspace).await {
        Ok(response) => {
            println!("Success! New scope: {}", response.scope_id);
            println!("Definition code:\n{}", response.code);
        }
        Err(e) => {
            println!("Inspection failed: {}", e);
            println!("This is expected if LSP's definition feature is not working");
        }
    }

    // Cleanup
    workspace.shutdown().await;
    std::fs::remove_file(test_file).ok();
    println!("\nDemo completed!");

    Ok(())
}
