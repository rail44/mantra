mod backend;

pub use backend::MantraBackend;

use anyhow::Result;
use tower_lsp_server::{LspService, Server};

/// Start the LSP server on stdio
pub async fn run_server() -> Result<()> {
    tracing::info!("Starting mantra LSP server");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(MantraBackend::new);

    Server::new(stdin, stdout, socket).serve(service).await;

    tracing::info!("LSP server stopped");
    Ok(())
}
