use anyhow::Result;
use jsonrpsee::core::client::{Client as RpcClient, ClientBuilder};
use tokio::io::BufReader as AsyncBufReader;
use tokio::process::Command;
use tracing::info;

use crate::lsp::transport::{StdioReceiver, StdioSender};

/// LSP connection that manages the RPC client
#[derive(Debug)]
pub struct LspConnection {
    pub client: RpcClient,
}

impl LspConnection {
    /// Create a new LSP connection by starting a language server process
    pub async fn new(command: &str, args: &[&str]) -> Result<Self> {
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

        // Build the RPC client
        let client = ClientBuilder::default().build_with_tokio(sender, receiver);

        Ok(Self { client })
    }
}
