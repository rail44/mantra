use anyhow::Result;
use jsonrpsee::core::client::{Client as RpcClient, ClientBuilder};
use tokio::io::BufReader as AsyncBufReader;
use tokio::process::{Child, Command};
use tracing::info;

use crate::lsp::transport::{StdioReceiver, StdioSender};

/// Create a new LSP client by starting a language server process
/// Returns a RpcClient that implements LspRpcClient trait methods
pub async fn create_lsp_client(command: &str, args: &[&str]) -> Result<(RpcClient, Child)> {
    info!("Starting LSP server: {} {:?}", command, args);

    let mut cmd = Command::new(command);
    for arg in args {
        cmd.arg(arg);
    }

    let mut process = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let stdin = process.stdin.take().expect("Failed to get stdin");
    let stdout = process.stdout.take().expect("Failed to get stdout");

    // Create transport components
    let sender = StdioSender::new(stdin);
    let receiver = StdioReceiver::new(AsyncBufReader::new(stdout));

    // Build the client
    let rpc_client = ClientBuilder::default().build_with_tokio(sender, receiver);

    Ok((rpc_client, process))
}
