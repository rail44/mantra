use anyhow::Result;
use mantra::config::Config;
use mantra::document::DocumentCommand;
use mantra::workspace::Workspace;
use std::path::PathBuf;
use tokio::sync::oneshot;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create workspace
    let root_dir = PathBuf::from(".");
    let test_file = "examples/test_data/simple.go";
    let config = Config::load(test_file)?;
    let mut workspace = Workspace::new(root_dir, config).await?;

    println!("Workspace created successfully");

    // Try to get a document actor
    let uri = format!(
        "file://{}",
        std::env::current_dir()?.join(test_file).display()
    );

    let document_sender = workspace.get_document(&uri).await?;
    println!("Document actor created for: {}", uri);

    // Test getting source
    let (tx, rx) = oneshot::channel();
    document_sender
        .send(DocumentCommand::GetSource { response: tx })
        .await?;

    match rx.await? {
        Ok(source) => {
            println!("Retrieved source ({} bytes)", source.len());
            println!("First 100 chars: {}", &source[..source.len().min(100)]);
        }
        Err(e) => {
            println!("Failed to get source: {}", e);
        }
    }

    // Shutdown
    workspace.shutdown().await;
    println!("Workspace shutdown complete");

    Ok(())
}
