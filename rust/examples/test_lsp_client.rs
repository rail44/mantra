use anyhow::Result;
use mantra::lsp::{
    Client as LspClient, DidOpenTextDocumentParams, HoverParams, InitializeParams, LspRpcClient,
    Position, TextDocumentIdentifier, TextDocumentItem,
};
use tracing_subscriber::fmt;

#[tokio::main]
async fn main() -> Result<()> {
    fmt::init();

    // Start gopls with notification support
    let client = LspClient::new("gopls", &[]).await?;

    // Initialize the client - direct trait usage
    let current_dir = std::env::current_dir()?;
    let workspace_root = current_dir.parent().unwrap().to_str().unwrap();

    println!("Initializing LSP client with workspace: {}", workspace_root);
    let init_params = InitializeParams {
        process_id: Some(std::process::id()),
        root_uri: Some(format!("file://{}", workspace_root)),
        capabilities: serde_json::json!({
            "textDocument": {
                "hover": {
                    "contentFormat": ["markdown", "plaintext"]
                },
                "synchronization": {
                    "didOpen": true
                }
            }
        }),
        workspace_folders: Some(vec![serde_json::json!({
            "uri": format!("file://{}", workspace_root),
            "name": "workspace"
        })]),
    };
    let init_result = LspRpcClient::initialize(client.rpc_client(), init_params).await?;
    println!("Server capabilities - Hover: {}", init_result.capabilities.hover_provider);
    if let Some(server_info) = &init_result.server_info {
        println!("Server: {} {:?}", server_info.name, server_info.version);
    }

    // Send initialized notification
    LspRpcClient::initialized(client.rpc_client()).await?;

    // Open the test document
    let test_file = format!("{}/examples/sample.go", workspace_root);
    let test_file_uri = format!("file://{}", test_file);
    let test_content = "package main\n\nfunc main() {\n\tvar x int = 42\n\tprintln(x)\n}\n";

    let did_open_params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: test_file_uri.clone(),
            language_id: "go".to_string(),
            version: 1,
            text: test_content.to_string(),
        },
    };
    LspRpcClient::did_open(client.rpc_client(), did_open_params).await?;

    // Wait for diagnostics to ensure the file is analyzed
    let timeout = std::time::Duration::from_secs(5);
    match client.wait_for_diagnostics_timeout(&test_file_uri, timeout).await {
        Ok(diagnostics) => {
            println!("Received {} diagnostics", diagnostics.diagnostics.len());
            for diag in &diagnostics.diagnostics {
                println!("  - {}: {}", match diag.severity {
                    Some(1) => "ERROR",
                    Some(2) => "WARNING",
                    Some(3) => "INFO",
                    Some(4) => "HINT",
                    _ => "UNKNOWN"
                }, diag.message);
            }
        }
        Err(e) => {
            println!("No diagnostics received: {}. Continuing anyway.", e);
        }
    }

    // Get hover information
    let hover_params = HoverParams {
        text_document: TextDocumentIdentifier {
            uri: test_file_uri,
        },
        position: Position {
            line: 3,
            character: 5,
        },
    };
    let hover_result = LspRpcClient::hover(client.rpc_client(), hover_params).await?;

    match hover_result {
        Some(hover) => {
            println!("Hover content: {:?}", hover.contents);
            if let Some(range) = hover.range {
                println!("Hover range: {:?}", range);
            }
        }
        None => println!("No hover information available"),
    }

    Ok(())
}
