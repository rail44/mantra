use anyhow::Result;
use mantra::lsp::Client;
use tracing_subscriber::fmt;

#[tokio::main]
async fn main() -> Result<()> {
    fmt::init();
    
    // Start gopls
    let client = Client::start("gopls", &[]).await?;
    
    // Initialize the client
    let current_dir = std::env::current_dir()?;
    let workspace_root = current_dir
        .parent()
        .unwrap()
        .to_str()
        .unwrap();
    
    println!("Initializing LSP client with workspace: {}", workspace_root);
    let init_result = client.initialize(workspace_root).await?;
    println!("Initialization result: {:?}", init_result);
    
    // Send initialized notification
    client.notify("initialized", serde_json::json!({})).await?;
    
    // Test hover request
    let test_file = format!("{}/examples/sample.go", workspace_root);
    
    // First, open the document
    client.notify("textDocument/didOpen", serde_json::json!({
        "textDocument": {
            "uri": format!("file://{}", test_file),
            "languageId": "go",
            "version": 1,
            "text": "package main\n\nfunc main() {\n\tvar x int = 42\n\tprintln(x)\n}\n"
        }
    })).await?;
    
    // Wait a bit for the file to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    
    // Test hover on variable 'x'
    let hover_result: serde_json::Value = client.request("textDocument/hover", serde_json::json!({
        "textDocument": {
            "uri": format!("file://{}", test_file)
        },
        "position": {
            "line": 3,
            "character": 5
        }
    })).await?;
    
    println!("Hover result: {:?}", hover_result);
    
    Ok(())
}