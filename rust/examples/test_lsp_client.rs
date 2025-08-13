use anyhow::Result;
use mantra::lsp::{
    create_lsp_client, DidOpenTextDocumentParams, HoverParams, InitializeParams, LspRpcClient,
    Position, TextDocumentIdentifier, TextDocumentItem,
};
use tracing_subscriber::fmt;

#[tokio::main]
async fn main() -> Result<()> {
    fmt::init();

    // Start gopls - minimal API
    let (client, _process) = create_lsp_client("gopls", &[]).await?;

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
    let init_result = LspRpcClient::initialize(&client, init_params).await?;
    println!("Initialization result: {:?}", init_result);

    // Send initialized notification
    LspRpcClient::initialized(&client).await?;

    // Open the test document
    let test_file = format!("{}/examples/sample.go", workspace_root);
    let test_content = "package main\n\nfunc main() {\n\tvar x int = 42\n\tprintln(x)\n}\n";

    let did_open_params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: format!("file://{}", test_file),
            language_id: "go".to_string(),
            version: 1,
            text: test_content.to_string(),
        },
    };
    LspRpcClient::did_open(&client, did_open_params).await?;

    // Wait for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Get hover information
    let hover_params = HoverParams {
        text_document: TextDocumentIdentifier {
            uri: format!("file://{}", test_file),
        },
        position: Position {
            line: 3,
            character: 5,
        },
    };
    let hover_result = LspRpcClient::hover(&client, hover_params).await?;

    println!("Hover result: {:?}", hover_result);

    Ok(())
}
