use anyhow::Result;
use jsonrpsee::core::client::{Client as RpcClient, ClientBuilder};
use std::sync::Arc;
use tokio::io::BufReader as AsyncBufReader;
use tokio::process::{Child, Command};
use tracing::info;

use crate::lsp::transport::{StdioReceiver, StdioSender};
use crate::lsp::NotificationHandler;

/// LSP connection that manages the process and RPC client
#[derive(Debug)]
pub struct LspConnection {
    pub client: RpcClient,
    pub process: Child,
    pub notification_handler: Arc<NotificationHandler>,
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

        // Create notification handler
        let notification_handler = Arc::new(NotificationHandler::new());

        // Create transport components with notification handler
        let sender = StdioSender::new(stdin);
        let receiver = StdioReceiver::with_notification_handler(
            AsyncBufReader::new(stdout),
            notification_handler.clone(),
        );

        // Build the RPC client
        let client = ClientBuilder::default().build_with_tokio(sender, receiver);

        Ok(Self {
            client,
            process,
            notification_handler,
        })
    }

    /// Shutdown the LSP process
    pub async fn shutdown(mut self) -> Result<()> {
        self.process.kill().await?;
        Ok(())
    }
}
